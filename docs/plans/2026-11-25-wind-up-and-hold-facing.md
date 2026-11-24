# Blueprint: A Swing Ends When Its Target Is Lost, and a Holding Hull Faces Its Target Between Shots (B-207, B-199)

This plan settles two registered bugs of `DESIGN.md` §4 against StarCraft II 5.0.14:

- **B-207: a wind-up whose target died on the tick before holds its unit for one more tick.** The combat phase of
  tick T kills the target and the reap phase removes it; on T + 1 the movement phase still finds the attacker in
  `WeaponPhase::Windup`, reads the swing with the weapon an idle unit would use (`wind_up_holds`) and, unless that
  weapon is `Moving`, stands the unit. Only the combat phase of T + 1 ends the swing, and then it starts nothing that
  tick. In 5.0.14 the attack is cancelled at once: the unit is free to walk and to attack again on the next game
  step.
- **B-199: between shots, the hull of a hull-weapon unit on Hold Position stays where it fired.** Pursuit aims the
  hull of a unit under an attack order in its band on every tick (`Pursuit::aim_if_in_range`); a holder has no
  pursuit, and the weapon phase turns only turrets between shots (`face_target_between_shots`). So a holder whose
  target walks across its front waits for a turn at the end of every cooldown. Every 5.0.14 legacy weapon faces its
  target while it cools down (`FaceTargetWhileInCooldown 1`).

The two are independent (§3.3) and land as two work packages on one branch, with records before and one
re-recording of the goldens on the final tree after.

A full plan is required (charter §6): both fixes move battle outcomes and golden replays. Neither changes the hashed
encoding (§5).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`a59368a3` (checksum schema 46).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (<role>, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- Numbers are provisional, next free at merge: bug rows from B-208, questions from Q0113.

**Questions and standing rules.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0103's standing guidance (charter §8): natural movement outweighs keeping close-matchup winners; a close battle that flips is restated with evidence; broken behaviour is a bug | Standing rule | §6.3, §8 |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S6 to S9 |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0109 part 2 (answered (a)): turrets face their target between shots | Closed | D-4 extends the same rule to the hull |
| The owner's guidance: no fitting to fixed seed sets; held-out seeds only confirm | Followed | §6, §7.4 |
| `docs/questions/open/` on `a59368a3`: Q0112 (attacker snapshot), unrelated | Read | — |
| No new owner question needed | — | §15 (a contingency draft, Q0113, not filed) |

---

## 1. Summary

| | Today (`a59368a3`) | After this plan |
|---|---|---|
| A wind-up whose bound target died (reaped) on tick T | Still `Windup` when T + 1's movement phase reads it; held unless the idle weapon is `Moving`; combat ends it on T + 1 and starts nothing that tick | Ended before T + 1's acquisition and movement by the combat phase's own test (D-1); the unit walks on T + 1 and may start its next swing on T + 1 |
| A wind-up whose target became unavailable at the start of the tick (boarded, housed, cloaked and unrevealed, out of sight) | As above: held for the tick, ended by combat | Ended in the same pass (D-1), one rule for both phases |
| A wind-up whose target lives and is available | Held unless `Moving` (turret wind-up D-6) | Unchanged |
| `wind_up_holds` with a gone target (direct phase calls only) | Reads the idle weapon (`SimWorld::idle_weapon_plane`) | Holds nothing (D-2) |
| Hull of a holder (or an immobile hull unit) between shots | Frozen from release until the cooldown ends | Turns towards the weapon's target at its stationary rate through backswing and cooldown (D-4) |
| Hull of a pursuer between shots | Aimed by pursuit in its band | Unchanged: the weapon does not turn a hull pursuit aims (D-4) |
| A turret between shots | Faces its target (fire-rules D-3) | Unchanged |
| Hashed state, schema | — | No new field; schema unchanged (46, or whatever `main` holds at merge) |
| Worlds that move | — | WP1: wherever a target dies or is lost mid-wind-up (§7.3; the earlier measurement moved 17 matrix battles, battles 31 and 33 and the showcase). WP2: worlds with a holding hull-weapon unit (at most 10 matrix battles; both goldens) |
| Size | — | M: four packages; WP1 S–M, WP2 S, WP0 and WP3 records and measurement |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [COMM] community or editor documentation; [INF] inferred; **verify** not
confirmed.

### 2.1 Sources

Game data is SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as
data. The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`.

| # | Fact | Data or source | Tag |
|---|---|---|---|
| S1 | The attack ability never acquires a dead unit | `CAbilAttack` default `AcquireFilters "-;Player,Ally,Neutral,Dead,Hidden,Invulnerable,Benign"` (`core` AbilData:115–118) | [DATA] |
| S2 | A weapon strikes only a target its player sees | `CWeapon` default `TargetFilters "Visible;-"` (`core` WeaponData:29); modelled by plan `2026-11-24-weapon-validation` D-3 as `StrikeContext::is_available` | [DATA] |
| S3 | Every legacy weapon faces its target while it cools down | `CWeaponLegacy` default `LegacyOptions FaceTargetWhileInCooldown 1` (`core` WeaponData:34); no layer clears it (`grep FaceTargetWhileInCooldown */WeaponData.xml`: only that line) | [DATA] |
| S4 | A unit turns in place at its `StationaryTurningRate`; the engine default is 720 °/s, structures 0 | `CUnit` default `StationaryTurningRate 720`, `TurningRate 720` (`core` UnitData:39–40); the structure default 0 (`core` UnitData:92–93); per-unit values in 182 rows of the layers; ours is `UnitArchetype::stationary_turn_rate` (plan `2026-11-05-turn-before-move` D-10) | [DATA] |
| S5 | Damage lands at the damage point; before it the attack has done nothing and can be cancelled | `DamagePoint 0.167` default (`core` WeaponData:17); the turret wind-up plan S3 | [DATA] |
| S6 | A weapon whose target dies before the damage point cancels its attack | SC2Mapster forum thread "splitting attacks when original target dies" (163428): "If that target unit dies before the projectile is launched, the weapon cancels it's attack", with a reply that the cancel ends the animation as well; read through a search result on 2026-11-24, the page itself redirects and was not fetched | [COMM], **verify** the exact game step (same step as the death, or the next) |
| S7 | After its target dies, a unit takes a new target without a pause of its own | SC2Mapster forum thread "trying to face its target" (162155): after the weapon's first attack killed its target, the unit switched to a new target and kept facing it for the rest of the weapon's cooldown; common play knowledge (overkill, target-firing micro) | [COMM], **verify** |
| S8 | A unit between shots faces its (new) target, turning its body when the weapon has no turret | S3 and S7 together: the unit in S7 is a hull unit ("stares at it for 1.5 s because the weapon is on cooldown") | [COMM] + [DATA] |
| S9 | A unit on Hold Position attacks enemies in reach without moving, turning to face them | Hold Position is a `CAbilMove` command (`core` AbilData:533–561), and the engine acquires for it (turret wind-up plan S12); S3 applies to its weapon like any other | [COMM], [INF] |
| S10 | `UninterruptibleDuration 32767.9997` on the legacy default | `core` WeaponData:38. Not a distinguishing fact (every legacy weapon), and it does not stop a cancel on target death (S6) or a stutter step (Liquipedia, "Micro (StarCraft)": attack-cancelling by a move before the damage point) | [DATA], not modelled (as the turret wind-up plan S13) |

### 2.2 The rules that follow

1. **A swing whose target can no longer be struck has ended** (S1, S2, S6). It deals nothing, charges no
   cooldown (as today, `step_windup`) and holds nothing. No phase that runs after the loss reads it as a swing.
2. **The unit is free on the next step** (S6, S7): it walks as its order says and may start a new swing at once
   against the target acquisition gives it. There is no penalty tick.
3. **Between shots a weapon faces its target, through whatever it is mounted on** (S3, S8): a turret by its turret
   rate (done, B-197), a hull weapon by turning the unit at its stationary rate (S4). Under an attack order our
   pursuit already does this for a mobile unit in its band; a holder, which no pursuit serves, must get it from the
   weapon (S9).
4. **The hull belongs to the weapon only when the weapon acts for an attack order or a hold** (turret wind-up plan
   D-5, kept): a weapon acting on its own initiative (`Mandate::OwnInitiative`) never turns the hull.

### 2.3 What the data does not settle

- **The step of the cancel (S6).** "At once" is read as: no phase after the loss treats the swing as live. Whether
  5.0.14 lets the unit start its next attack on the same game step as the death or on the next one is not in the
  data. Ours: the next tick (D-1 runs at the start of T + 1), which is the earliest our phase order allows for a
  death in T's combat phase. Default chosen, **verify**.
- **Losing sight of the target mid-wind-up (S2).** The data says a weapon needs a visible target; whether the
  engine cancels a started attack when the target leaves sight before the damage point is not stated. Ours already
  cancels it in the combat phase (`step_windup` via `is_available`, weapon-validation WP5); D-1 only moves that test
  before movement for a loss known at the start of the tick. **verify**, no change of meaning.
- **Turning rate between shots.** Whether the hull turns at `StationaryTurningRate` or `TurningRate` between shots
  is not stated; a holder stands, so ours uses the stationary rate (`reset_hull_turn_budgets`), as the ready step
  already does. [INF]

### 2.4 Neighbouring rules: in or out

| Rule | Decision |
|---|---|
| A backswing bound to a target that died | **Out, unchanged.** The backswing is the follow-through of a shot that was fired; it does not hold the unit (`wind_up_holds` reads only `Windup`) and ends by its own count. No source says a death ends it |
| `RotatingToTarget` towards a target that died | **Out, unchanged.** It is not bound (the combat phase reads the ordered target every tick and returns it to `Ready`); only the legacy anchoring reads it, for one tick |
| A target lost during the movement phase of the same tick (it walks into fog, boards) | **Unchanged:** the movement phase has already run on a live swing; the combat phase ends it that tick, as today. Inherent to the phase order; 5.0.14 resolves within a step too |
| The hull of a pursuer between shots | **Unchanged** (D-4 alternative): pursuit aims it at the goal read at the start of the movement phase; re-aiming it at the target's end-of-movement position would change every chase in the matrix for a sub-degree refinement no source asks for |
| `Slowing` against `Stopped` (B-178), `NoDeceleration` (B-181) | **Out**, still registered |
| The chase read from pursuit (fire-rules WP6), the last-seen pursuit (weapon-validation WP7, B-195) | **Out**, other plans |
| The contact solver's enemy-contact rules (branch `enemy-contact-plan`) | **Out**, another planner's |

---

## 3. Diagnosis (on `a59368a3`, read from the code)

### 3.1 B-207

- **Phase order** (`SimWorld::step_into`, `crates/sim_core/src/world.rs`): commands, production, tasks,
  `update_vision`, `acquire_targets`, `trigger_attack_dashes`, `step_movement_and_steering`,
  `resolve_collisions_and_push`, `step_weapons_and_projectiles`, `count_down_cast_locks`,
  `reap_dead_and_emit_shatter`. Damage is dealt in the combat phase; the dead are removed at the end of the tick.
- **Who ends a swing.** Only `step_windup` (`crates/sim_core/src/combat.rs`): `target_opt` is the swing's bound
  target if the combat snapshot has it and `StrikeContext::is_available` passes
  (`advance_weapon_cycles`, `crates/sim_core/src/combat_resolution.rs`); otherwise the phase becomes `Ready` and the
  function returns false, so **no new swing starts on that tick** even when another target is in band and the
  cooldown is 0. When no weapon resolves for the gone target's plane, `advance_weapon_cycles` resets the swing
  without stepping the weapon at all.
- **Who reads a stale swing in between.** On T + 1, before the combat phase:
  - `SimWorld::wind_up_holds` (`movement/locomotion.rs`), called by `walking_goal` for both pipelines before pursuit:
    the bound target is absent, so it resolves the weapon against `SimWorld::idle_weapon_plane` and holds unless
    that weapon is `Moving`. This is B-207's hold.
  - `SimWorld::is_anchored_in_combat` (legacy pipeline): a `Windup` anchors the unit, so friends do not push it.
  - `Pursuit::aim_if_in_range` presents `AnimState::Attack` for `Windup` (not reached: the hold returns first).
  - The render snapshot (`world/snapshot.rs`, `published_swing`, `bubble_instance`): a `Windup` is published as an
    attack in progress on T (the frame of the death) and is gone on T + 1 only after combat.
  - Acquisition (`target_acquisition.rs`) does not read the weapon phase.
- **So B-207 has two halves:** the hold on T + 1 (movement) and the lost tick before the next swing (combat). The
  naive fix measured by turret wind-up WP3 (Amendment 3, `~/scratch/turret-wp3/world-wp3b`,
  `locomotion-variantA.rs`) removed only the first half: `wind_up_holds` held nothing for a gone or dead target.

### 3.2 B-199

- `step_entity_weapon`'s `Cooldown` arm and `step_backswing` call `face_target_between_shots`, which returns at once
  for a hull weapon ("a hull weapon's hull is left to pursuit"). The ready step turns the hull
  (`step_ready_weapon` → `slew_towards` → `Entity::turn_hull_towards`) only once the cooldown is 0.
- `walking_goal` breaks out for `is_on_hold() || is_immobile` before the order is classified, so neither a holder
  nor an immobile unit ever reaches `Pursuit::aim_if_in_range`; a holder coasts (`coast`), which does not aim.
- The hull budget (`reset_hull_turn_budgets`) gives a standing unit `stationary_turn_rate` per tick; the movement
  phase spends first, the weapon the rest. A holder that does not glide keeps the whole budget for the weapon.
- `turret_tracking_tests::between_shots_a_hull_weapon_does_not_turn_its_hull` pins today's rule (fire-rules R-7).
- **Immobile hull units.** Every armed structure in `structures.ron` has a turret; the immobile modes of units
  (sieged tank, burrowed Lurker) have turrets. Whether any immobile mode carries a hull weapon (for example the
  mode that `world/abilities.rs` makes immobile while cloaked) is WP0's census (**verify**); D-4 covers it either way.

### 3.3 Independence

B-207 changes when a `Windup` ends; B-199 changes what a hull does in `Backswing` and `Cooldown`. Neither reads the
other's state, they touch different functions, and a world moved by one is traced to its own rule. They are two
packages so that each package's first differences have one cause (§8 rule 2).

---

## 4. Design decisions

### D-1. A swing whose target is lost ends before anything reads it (B-207)

A new combat-side pass, `SimWorld::end_lost_swings`, runs in `step_into` **after `update_vision` and before
`acquire_targets`**. For every entity in `WeaponPhase::Windup { target_id, .. }` it applies the combat phase's own
test: the bound target exists, and the target snapshot of it passes `StrikeContext::is_available` for the
attacker's owner. If the test fails, the phase becomes `Ready` (no shot, no cooldown charged, as `step_windup`
does). Nothing else changes: the order, `current_target`, `engaged_target` and the cooldown are left to acquisition
and the combat phase.

- **One test, two callers.** The snapshot of a target (`TargetSnapshot`: position, radius, plane, damageable,
  cloaked, attack-revealed) is built by one function, `SimWorld::target_snapshot(entity)`, which
  `capture_target_snapshots` and the pass both call; the availability test stays `StrikeContext::is_available`. A
  helper `swing_target_live(context, attacker, snapshot) -> bool` is the single statement of rule 1 that the pass
  and `step_windup`'s caller share.
- **Why before acquisition and movement.** Vision for the tick is current (`update_vision` ran), the dead were
  reaped at the end of T, and every later reader (acquisition, movement, legacy anchoring, the combat phase) sees
  the honest phase. The combat phase of T + 1 then finds `Ready` and steps the ready path against the target
  acquisition chose that tick: the second half of B-207 is fixed without any combat-side special case.
- **Why not in the reap phase.** It would cover death only, not a target that boarded, was housed, or was lost to
  sight between T's combat and T + 1's movement; and the vision of T + 1 is not yet known there.
- **Why not a phase-order change (combat before movement).** It would change every weapon cadence and chase in every
  world for a one-tick defect.
- **Why not only a validity check in `wind_up_holds`** (the measured naive alternative, extended to availability).
  It fixes the hold but leaves the stale phase for the combat phase to throw away a tick later, so the next swing
  still waits a tick (rule 2), and leaves the legacy anchoring and the snapshot reading a swing that has ended. It
  also writes rule 1 a second time, in the movement crate's terms.
- **The combat phase keeps its own test**, for a target lost during the same tick's movement phase (§2.4).
- **Allocation.** The pass collects the ids to end into a `Vec<EntityId>` kept in `CombatWorkspace` (cleared, not
  freed), because it must read the target while it changes the attacker; no steady-state allocation. Cost: one pass
  over the entities with one map lookup per entity in `Windup`, O(N log N). It carries a
  `// PERFORMANCE RATIONALE:` block only if it departs from the plain two-pass form (it is not expected to).

### D-2. `wind_up_holds` holds nothing for a swing whose target is gone

With D-1 in place, a full tick never reaches the movement phase with a `Windup` whose target is absent. Direct
phase calls (`step_movement_and_steering` in tests) still can. `wind_up_holds` returns `false` when the bound target
is absent, instead of reading `idle_weapon_plane`; its doc comment states that the pass of D-1 ends such a swing
before a full tick's movement phase. `idle_weapon_plane` stays `pub(crate)` only if another caller remains
(**verify** in WP1; otherwise it returns to private, with its doc's Amendment 2 sentence removed). The rule for a
live target (hold unless the swing's weapon is `Moving`) is unchanged.

### D-3. Records for B-207

`stutter_step_tests::a_wind_up_whose_target_is_gone_holds_as_its_idle_weapon_says` is restated as
`a_wind_up_whose_target_is_gone_holds_nothing` (§6.1 A-1). B-207's row is marked fixed in WP3.

### D-4. A hull faces its target between shots when no pursuit aims it (B-199)

`face_target_between_shots(entity, weapon, target_opt, mandate)` gains the mandate. It turns:

- a **turret** towards the target at the turret rate, as today, unless its weapon locks it (D-2 of fire rules);
- a **hull** towards the target through `Entity::turn_hull_towards` (what is left of this tick's hull budget, the
  stationary rate for a standing unit, 0 for a structure), when **`mandate` is `Mandate::AttackOrder`** (rule 4) and
  **the unit's hull is not aimed by pursuit**: it holds (`Entity::is_on_hold`) or is immobile (`is_immobile`). The
  condition is one predicate, `Entity::hull_aimed_by_weapon()`, documented with the reason (pursuit's
  `aim_if_in_range` aims every other unit under an attack order in its band; `walking_goal` breaks out for these two
  before pursuit).

Callers: the `Cooldown` arm of `step_entity_weapon` and `step_backswing` (after its ready step started nothing),
both of which already have `mandate`. In `step_backswing` the lock condition reads `weapon.has_turret &&
weapon.locks_turret_while_firing` for the turret case only; the lock never holds a hull (fire-rules D-2's own words).
The engaged-arc bookkeeping is untouched: the next swing's arc test is the ready step's, as for a turret.

**Alternative considered: every hull under an attack order.** Rejected (§2.4): pursuit already aims a pursuer's hull
in its band; adding the weapon's turn after movement would re-aim it at the target's end-of-movement position with
the leftover budget on every tick of every chase in band, a world-wide change no source asks for, and a second aim
of one hull in one tick.

**Alternative considered: a movement-side rule in the hold branch of `walking_goal`.** Rejected: the movement
phase would have to resolve the weapon, its target and its phase, which the combat phase already has; the turret
half of the same data rule lives in `face_target_between_shots`, so the hull half belongs beside it.

### D-5. No field, no data

`FaceTargetWhileInCooldown` stays unmodelled as a field (fire-rules D-1: no weapon clears it). The turn rate is the
existing `stationary_turn_rate`. No catalog change; no identity in Rust (C5).

### D-6. Records

- `DESIGN.md` §4: B-207 and B-199 point here (WP0) and are marked fixed (WP3). New rows, if any, from **B-208**.
- `docs/plans/2026-11-23-turret-wind-up.md` and `2026-11-24-fire-rules.md`: one pointer line each in their
  Amendment 3 and As-built R-7 entries (WP3).
- Evidence: `docs/evidence/2026-11-25-wind-up-and-hold-facing.md` (new, WP0 to WP3).

---

## 5. Determinism and checksum

- **No new hashed field.** Both rules change only `weapon_phase` and `heading`, which are hashed already. The
  checksum schema stays where `main` has it at merge (46 today; 47 once weapon-validation WP7 merges). If an
  implementation finds it must add hashed state, it takes the next free schema at merge (48 if WP7 has 47) and
  records it in an amendment.
- **Order of the pass.** Entities in `EntityId` order (the map's order), reading the snapshot of the target as it
  stands after `update_vision`; no randomness; no float (C7).
- **Goldens.** `golden_replay.ron`, `golden_replay_orders.ron` and `DEFAULT_SANDBOX_CHECKSUM`
  (`crates/sim_fixtures/tests/fixture_tests.rs`) are re-recorded **once, in WP3, on the final tree**, after both
  behaviour packages, with the proof on four builds (native debug and release, `wasm32-wasip1` debug and release;
  the turret wind-up WP3 `proof.sh`). The header comment of each re-recorded file names this plan and the first
  differing checkpoint of each package (§7.2). WP1 and WP2 commits are not merge points: their golden tests are
  expected to fail and the hand-back says so.
- **Throughput.** `cargo bench -p sim_core --bench throughput` on WP0's tree and on the final tree, release, at the
  load the guide names: every scenario at **50 ticks per second or more** (the task's bar, above the charter's 25),
  and the instruction count of each within 1 % of WP0's (the pass is one loop; the hull turn is one call per
  holder between shots). A scenario below 50 ticks/s is stop rule 7.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure from a seed. Constructed tests find units and weapons by property (`has_turret`,
`allowed_movement`, `stationary_turn_rate`, plane), never by slug.

### 6.1 Constructed tests, B-207 (WP1)

- **A-1 (no hold for a lost swing).** A unit in `Windup` whose bound target is killed and reaped on tick T walks on
  T + 1 under a walking order (`AttackMove`, `Move` queued behind, task goal), for every `AllowedMovement`, both
  mounts and both pipelines (`ContactRules::pipeline` switched in the catalog, as `push_priority_tests` does).
  Replaces the B-207 pin test. The direct-phase case (D-2) is asserted too.
- **A-2 (no lost tick).** In the same scene with a second enemy in band and aligned, cooldown 0: a swing against the
  second enemy starts on T + 1 (today: T + 2). With the cooldown not yet 0, it starts the tick the cooldown ends (no
  later than today).
- **A-3 (any loss known at the start of the tick).** A swing whose target, between T's combat and T + 1's
  movement, (a) boards a transport, (b) is housed by a structure it gathers at, (c) becomes cloaked unrevealed, or
  (d) is no longer seen by the attacker's player (vision update), is `Ready` before T + 1's movement phase and holds
  nothing. Each case is constructed with the public phase calls in `step_into`'s order.
- **A-4 (a live swing is unchanged).** Turret wind-up P-10
  (`a_wind_up_holds_its_unit_unless_its_weapon_fires_while_moving`) and the chase tests of `turret_tracking_tests`
  pass unchanged; `weapon_cadence_tests`, `multi_strike_timing_tests` and `stutter_step_tests` pass except A-1's
  restated test.
- **A-5 (nothing charged).** A lost swing charges no cooldown and no random delay draw (the RNG position after the
  tick equals a world where the swing never started).
- **A-6 (readers agree).** On T + 1 the legacy pipeline's `is_anchored_in_combat` does not anchor the unit by its
  phase; the render snapshot of T + 1 publishes no swing for it (`attack_swing_snapshot_tests`).

### 6.2 Constructed tests, B-199 (WP2)

- **B-1 (a holder faces its target).** The scene of `between_shots_a_hull_weapon_does_not_turn_its_hull` (restated
  as `between_shots_a_holding_hull_faces_its_target`): on every backswing and cooldown tick with a target, the hull's
  bearing error to the target does not grow by more than the target's own angular motion that tick, and the hull
  turns by at most `stationary_turn_rate` per tick. Its next swing starts no later than today's, and strictly
  earlier in the scene where the target crosses the front.
- **B-2 (pursuers unchanged).** A unit under `AttackTarget`, mobile, standing in its band while its target circles
  it: its heading on every tick equals the heading on WP1's tree (the weapon does not turn a hull pursuit aims).
- **B-3 (own initiative never turns a hull).** Turret wind-up P-4 passes unchanged.
- **B-4 (structures and stationary rate 0).** A scratch immobile hull weapon with `stationary_turn_rate` 0 never
  turns; with a positive rate it turns as B-1.
- **B-5 (turrets unchanged).** Every test of `fire_rules_tests` and `turret_tracking_tests` on turrets passes
  unchanged, the Hellion's lock included; a scratch hull weapon with `locks_turret_while_firing` (if validation
  admits it, **verify**) still turns its hull.
- **B-6 (no target, no turn).** A holder whose target is gone or unavailable keeps its heading between shots.

### 6.3 Behaviour checks on measured worlds (both packages, pre-registered)

Computed by the measurement of §7 against the predecessor tree (WP1 against WP0's tree, WP2 against WP1's), on the
development seeds, per world family (matrix, battle 31, battle 33, showcase, showcase effects):

- **W-1 No unit stands while able to fight.** A scratch probe (§7.1 probe (a)) counts, per world, unit-ticks where
  an alive armed unit has an available enemy in its weapon band, its weapon is `Ready` with cooldown 0, it starts
  no swing and does not rotate, and its own motion is zero. The count must not rise. After WP1 the probe's count of
  **stale holds** (a movement-phase hold of a `Windup` whose bound target is absent or unavailable at the start of
  the tick) is **0** in every world.
- **W-2 No more stall give-ups.** The sum of `stall_giveups` (matrix CSV) per family does not rise; `blocked_attacker_ticks`
  per battle rising by more than 5 % is traced (§8 rule 3).
- **W-3 Firing rate does not drop.** Shots released per 1,000 alive unit-ticks (the trace's `shots_released` over
  `alive`), summed over the seeds of each moved battle: the aggregate over all moved matrix battles does not drop,
  and a single battle dropping by more than 1 % is traced (§8 rule 3).
- **W-4 (B-207 specific).** Over every world, swings started on the tick after a lost swing rise (the second half
  of the fix is visible) and `cancelled_target_gone` changes only by divergence (the cancels still happen, in the
  pass). The mean end tick of each battle family does not rise by more than 1 %.
- **W-5 (B-199 specific).** For holding hull-weapon units (probe (b)): the ticks from the end of a cooldown to the
  start of the next swing (the ready step's turn wait) fall in aggregate, and no holder's hull turns while it has no
  available target.

### 6.4 Close battles (fixed in WP0, before any behaviour change)

WP0 computes and commits to the evidence file, from WP0's baseline and nothing else, the **close-battle list**:

1. every matrix battle in which, over its 20 baseline runs (seeds of record and the 16 development seeds), the side
   that wins fewer runs wins at least 2 (10 %);
2. battle 31 and battle 33 if the same share holds over their 100 and 64 baseline seeds;
3. every battle that a closed owner question or a fixture comment already names close or restated under §8
   (battles 26, 33, 38a, 38b per Q0103 and Q0044, and any other WP0 finds by `grep -n "close\|restated"
   crates/sim_core/tests/fixtures/matchup_matrix.ron`);
4. no other. A battle carrying `known_bug` (28, B-192) is not close; it is reviewed (§8 rule 5).

The list is not edited after WP1 starts.

### 6.5 Battles that move

"Moves" in this section means its **outcome** moves: on some measured seed the winner changes or the expectation
fails where it held. Byte-level differences with the same outcomes are expected (§7.3), recorded and sampled.

- A **close** battle whose outcome moves is restated under charter §8 with evidence: the first differing tick of one
  flipped seed, traced to the entity and rule (`firstdiff.py` and a per-unit `matchup_trace --battle … --seed …`),
  and, if the claim's range breaks, the expectation re-derived over seeds of record and 1 to 20 with a fixture
  comment citing this plan. No range is widened without that trace.
- **Any other battle** whose outcome moves is a **stop for review** (§8 rule 4).
- The held-out seeds (§7.4) only confirm; nothing is restated or tuned from them.

### 6.6 Shipped-world predictions (recorded before measuring, not thresholds)

- WP1 moves, at least, what the naive alternative moved on `e5f5c565` (17 matrix battles, battles 31 and 33 on most
  seeds, the showcase on seeds 1 to 8), plus worlds where the earlier next swing matters; the two expectation misses
  that alternative produced (26 seed 13, 36a seed 2) are expected again; 26 is close, 36a's status is set by §6.4.
- WP2 can move only the matrix battles with a hull-weapon holder. Read from the fixture's `HoldPosition` sides: 11a
  and 37a, 37b, 38a, 38b (Marine analogue), 11b (Marauder), 13 (Zerglings), 19 (Brood Lord), 20 (Roach) and 41
  (Void Ray); 12c holds a turreted Colossus and stays byte-identical. A battle moves only where a holder's target
  changes bearing between shots (melee Zerglings and a lone Roach may not). Both goldens may move (`red_psi` holds
  from tick 640, `red_ledge` from tick 0; their unit types are WP0's census). No outcome is expected to change;
  holders fire no later than before.

---

## 7. Measurement

### 7.1 Probes (WP0, scratch, not committed)

Patches applied to a scratch copy of WP0's tree, built once (`~/scratch/wind-up-hold/`), logging to CSV:

- **(a) Stale holds and standing-while-able** (W-1): in `walking_goal`, every hold by `wind_up_holds` is classified
  live, gone, dead or unavailable (boarded, housed, cloaked, unseen); after each tick, the standing-while-able count.
- **(b) Holder turn waits** (W-5): for each hull-weapon holder, the bearing error to its target at the end of each
  cooldown and the ticks until its next swing.
- **(c) Attribution build.** The naive alternative (`~/scratch/turret-wp3/locomotion-variantA.rs` ported to WP0's
  tree: movement half only) measured like a package, so that WP1's moves can be split into the hold half and the
  next-swing half (§7.3).
- **(d) Census.** Immobile modes with a hull weapon; matrix battles with a hull-weapon holder; the goldens' holders.

### 7.2 Commands

The turret wind-up plan's scripts, copied to `~/scratch/wind-up-hold/scripts/` (`measure.sh`, `compare.sh`,
`firstdiff.py`, `proof.sh`, `client.sh`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/wind-up-hold/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/wind-up-hold/bin-<wp>/
~/scratch/wind-up-hold/scripts/measure.sh ~/scratch/wind-up-hold/bin-<wp> <tree> ~/scratch/wind-up-hold/world-<wp>
~/scratch/wind-up-hold/scripts/compare.sh ~/scratch/wind-up-hold/world-<prev> ~/scratch/wind-up-hold/world-<wp>
```

`measure.sh` runs the matrix (seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds), the showcase and
the showcase effects scenario on the default seed and seeds 1 to 8 (6,000 ticks). One at a time across worktrees,
in the background, never beside a `cargo gate` (charter §7). The client skirmish by its stage log (`client.sh`).

### 7.3 Expected moves

| World | WP1 (B-207) | WP2 (B-199) |
|---|---|---|
| Matrix | Moves where a target dies mid-wind-up: most battles with hull-weapon units; at least the 17 of the naive alternative. Each moved battle's first difference classified H (hold), N (next swing a tick earlier) or V (a loss other than death), with probe (c) separating H from N | Moves where a hull-weapon unit holds (§6.6) |
| Battles 31, 33 | Move on most seeds (Marauders, Stalkers, Marines, Zealots are hull weapons) | Byte-identical (no holder) |
| Showcase, default seed and 1–8 | Moves on seeds 1–8 as before; the default seed **verify** | Byte-identical unless WP0's census finds a holder |
| Showcase effects | May move (tanks and Hellions under attack-move) | Byte-identical unless a holder (census) |
| Goldens, sandbox | Expected to move; not re-recorded until WP3 | Both goldens move (holders); re-recorded once in WP3 |
| Client skirmish stage log | Identical or traced | Identical |

### 7.4 Held-out seeds (confirmation only)

Run **once**, in WP3, on WP0's tree and on the final tree, after every development-seed result is settled:
matrix seeds 7001 to 7016 (`--all-battles --seed 7001 … --seed 7016`), battles 31 and 33 on seeds 7001 to 7064, the
showcase and showcase effects on seeds 9 to 16. They must show W-1 to W-5's directions and no outcome move in a
battle outside §6.4's list. A contradiction stops the plan for review (§8 rule 6); nothing is tuned or restated from
them.

---

## 8. Stop rules

1. **WP0 is records only.** No behaviour change; the close-battle list and the probes' baseline are committed (the
   list) or stored (the probes) before WP1's first build.
2. **One cause per package.** Every first difference in a world WP1 moves shows a swing ended by D-1 (class H, N or
   V); every one WP2 moves shows a holder's hull turned between shots. Any other first difference stops the package
   until traced; a second cause is registered (from B-208) and the package lands only if it is separable.
3. **A behaviour check fails** (W-1 to W-5, §6.3): stop and trace. A failure explained by divergence after the
   first difference (a later tick, a different fight) is recorded with its trace; one explained by the rule is a
   stop for review.
4. **A non-close battle's outcome moves** (§6.5): stop for review, with the trace. It is not restated by the
   implementer.
5. **A `known_bug` battle's state changes** (28 starts to hold its claim, or the marker's test fails): stop for
   review; it may mean B-192 changed, which belongs to the solver follow-up.
6. **The held-out run contradicts the development seeds** (§7.4): stop; report both. No iteration on held-out
   seeds.
7. **Throughput below 50 ticks/s on any scenario, or instructions up more than 1 %** (§5): stop and profile.
8. **A stall, or a unit standing while able to fight for a reason other than its own live wind-up** (W-1): stop
   (charter §8: broken behaviour is a bug, never restated).
9. **No tuning constant.** The plan adds none; a fix that seems to need one is a stop.
10. **Overlap.** WP1 does not start while `crates/sim_core/src/movement/locomotion.rs` is changed by an unmerged
    branch near `walking_goal` (weapon-validation WP7 is: `last_seen_goal`); it starts from `main` after that merge.
    Neither package touches the contact solver's enemy-contact rules.

---

## 9. Client consequences

- After WP1, a unit whose target dies mid-wind-up leaves its attack pose on the next frame and walks or turns to its
  next target at once; the death frame itself still shows the wind-up (the snapshot of T). No client code.
- After WP2, a holding Marine, Stalker or Viking analogue visibly turns to follow a moving target between shots.
  Presentation follows `heading` as today. No client code; the package gate runs the client tests its path rules
  select.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | B-207 and B-199 point here; marked fixed; any B-208+ |
| `docs/plans/README.md` | plan, WP3 | Row |
| `docs/evidence/2026-11-25-wind-up-and-hold-facing.md` (new) | WP0–WP3 | Baseline, close-battle list, probes, comparisons, traces, held-out, throughput |
| `crates/sim_core/src/world.rs` | WP1 | `end_lost_swings` in `step_into` after `update_vision`; the phase list in the docs |
| `crates/sim_core/src/combat_resolution.rs` | WP1 | `end_lost_swings`, `target_snapshot`, `swing_target_live`; `capture_target_snapshots` and `advance_weapon_cycles` call them; the reusable id list in `CombatWorkspace` |
| `crates/sim_core/src/movement/locomotion.rs` | WP1 | D-2 in `wind_up_holds` and its docs |
| `crates/sim_core/tests/stutter_step_tests.rs` | WP1 | A-1, A-2; the pin test restated |
| `crates/sim_core/tests/weapon_cadence_tests.rs` or `target_selection_tests.rs` | WP1 | A-3, A-5 (**verify** the closest subject file) |
| `crates/sim_core/tests/attack_swing_snapshot_tests.rs`, `push_priority_tests.rs` | WP1 | A-6 |
| `crates/sim_core/src/combat.rs` | WP2 | D-4 in `face_target_between_shots`, its callers, docs |
| `crates/sim_core/src/entity.rs` | WP2 | `hull_aimed_by_weapon` and its unit test |
| `crates/sim_core/tests/turret_tracking_tests.rs`, `hull_turning_tests.rs` | WP2 | B-1 to B-6; the R-7 pin test restated |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | WP3 | Re-recorded once (§5) |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP3, only under §6.5 | A close battle restated with its trace |
| `docs/plans/2026-11-23-turret-wind-up.md`, `2026-11-24-fire-rules.md` | WP3 | Pointer lines |

`crates/gate/suites.ron` needs no change: every test is in an existing target (**verify** in WP1).

---

## 11. Work packages (commit order)

One branch from `main` (after weapon-validation WP7 merges), one worktree,
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`; one commit per package; failing tests first, with the
red run pasted in the hand-back; `cargo gate quick` and the tests the change needs; one `cargo gate package` by a
verifier on the final tree (Q0061, charter §4.3). No `unwrap`; `expect` only with `invariant:`; no catalog identity
in production Rust. Check `docs/questions/open/` before the last commit.

### WP0: records, baseline, close battles, probes (S, docs and scratch)

- `DESIGN.md`: B-207's and B-199's rows point here.
- Baseline measurement of §7.2 on WP0's tree; probes (a) to (d); the close-battle list (§6.4) committed to the
  evidence file with the command that derived it. Throughput baseline.

### WP1: a swing ends when its target is lost (S–M, `sim_core`; B-207)

- D-1, D-2; A-1 to A-6. Measure against WP0, classify every moved world's first difference (H, N, V) with probe (c);
  W-1 to W-4; §6.5 for outcomes. Goldens not re-recorded.

### WP2: a holding hull faces its target between shots (S, `sim_core`; B-199)

- D-4; B-1 to B-6. Measure against WP1; W-1 to W-3 and W-5; §6.5.

### WP3: confirmation and records (S)

- Goldens and sandbox re-recorded once, proved on four builds; held-out run (§7.4); throughput (§5); `DESIGN.md`
  rows fixed; pointers in the two parent plans; README row; As built.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Unit | `hull_aimed_by_weapon` for every order shape and the immobile flag; `swing_target_live` for each loss | `entity.rs`, `combat_resolution.rs` `#[cfg(test)]` |
| Integration | A-1 to A-6, B-1 to B-6 | §10's test files (charter subjects "weapon timing", "turrets") |
| Determinism | Goldens and sandbox on four builds | `golden_replay_tests.rs`, `fixture_tests.rs` (WP3) |
| Worlds | W-1 to W-5, §6.5 | Evidence file |
| Throughput | §5 | `cargo bench -p sim_core --bench throughput` |

---

## 13. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | WP1's reach is wide (every death mid-wind-up), so outcome moves are likely in close battles | §6.4 is fixed first; §6.5 separates restatement from review |
| R-2 | The earlier next swing (half N) raises damage output slightly and shortens battles | W-3 and W-4 expect it; probe (c) attributes it |
| R-3 | Ending a swing on a vision loss known at tick start cancels some swings that today survive because the target walks back into sight during movement | Class V counted; a V-class first difference in a moved outcome is traced and reported. S2 supports the stricter reading |
| R-4 | Holders turning between shots changes which enemy is in arc first and so the order of kills | B-1 asserts no later swing; outcomes judged by §6.5 |
| R-5 | Merge conflict with weapon-validation WP7 in `locomotion.rs` | §8 rule 10 |

---

## 14. Out of scope and decisions recorded (Q0002)

- Backswing and `RotatingToTarget` bound to a lost target (§2.4): unchanged.
- The hull of a pursuer between shots (D-4 alternative): unchanged; not registered (no source asks for it).
- A target lost during the movement phase of the same tick: unchanged (§2.4).
- `UninterruptibleDuration` (S10): not modelled.
- B-178, B-181, fire-rules WP6, weapon-validation WP7, the contact solver's enemy-contact rules: other plans.

---

## 15. Owner question

None is needed: rule 1 is data (S1, S2) and a sourced community report (S6); rule 3 is data (S3, S4) and the
owner's own answer for turrets (Q0109 part 2 (a)). The step of the cancel (§2.3) is a default with a **verify**,
like the turret wind-up plan's.

**Contingency draft, not filed.** File it only if stop rule 4 fires and the trace shows the flip is caused by the
rule itself, not by divergence. Number provisional (next free at filing, Q0113 or later).

```markdown
# Q0113: A unit's target dies just before it fires. Should it walk on and fire again at once, even though battle <N> changes winner?

- **Asked:** 2026-11-2X
- **Blocking:** no
- **Default until answered:** (a). The fix stays on its branch, unmerged, until answered; other work continues.

## Question

In StarCraft II, when a unit is about to fire (the short "wind-up" before the shot) and its target dies, the attack
is cancelled at once: the unit can move and attack something else on the very next step. Space Scuffle instead kept
the unit standing for one extra step (bug B-207). Fixing it changes the winner of battle <N> ("<claim>") on <k> of
24 runs. The battle was not on the list of close battles fixed before the change; the trace is in
`docs/evidence/2026-11-25-wind-up-and-hold-facing.md` §<n>.

- **(a) Keep the fix and restate battle <N>'s claim** to what is measured, with the trace. Recommended: it is the
  StarCraft II behaviour, and the old result depended on units losing a step after every kill.
- **(b) Keep the fix and mark the battle as a known bug** to be investigated, if you believe the old winner is what
  StarCraft II gives.
- **(c) Do not merge the fix.**

## Answer

```

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/` (`5444e12f13`): `core` AbilData:115–139 (`CAbilAttack`), `core`
  WeaponData:3–39 (`CWeapon`, `CWeaponLegacy`), `core` UnitData:23–93 (turning rates), `grep -c
  StationaryTurningRate */UnitData.xml`.
- Community sources searched on 2026-11-24 (web search; the SC2Mapster pages redirect and were read from the search
  results only): SC2Mapster forum threads 163428 ("splitting attacks when original target dies") and 162155
  ("trying to face its target"); Liquipedia "Micro (StarCraft)". No source defines `FaceTargetWhileInCooldown` or
  `UninterruptibleDuration` in words.
- Code read on `a59368a3`: `world.rs` (`step_into`, `reset_hull_turn_budgets`); `combat.rs` (`step_entity_weapon`,
  `step_windup`, `step_backswing`, `face_target_between_shots`, `step_ready_weapon`, `slew_towards`, `Mandate`);
  `combat_resolution.rs` (`StrikeContext::is_available`, `capture_target_snapshots`, `advance_weapon_cycles`,
  `ordered_target`, `idle_weapon_plane`); `movement/locomotion.rs` (`walking_goal`, `wind_up_holds`,
  `is_anchored_in_combat`, `Pursuit::aim_if_in_range`); `entity.rs` (`turn_hull_towards`, `is_on_hold`);
  `trace/mod.rs` and `trace/activity.rs` (the metrics); the matrix fixture's `HoldPosition` sides.
- The naive alternative and its measurement: `~/scratch/turret-wp3/locomotion-variantA.rs`,
  `~/scratch/turret-wp3/world-wp3b/`, the turret wind-up evidence file's last section.
- Branches checked for overlap: `weapon-validation-wp7` (changes `walking_goal`'s neighbourhood, `last_seen_goal`);
  `enemy-contact-plan` (not touched).
