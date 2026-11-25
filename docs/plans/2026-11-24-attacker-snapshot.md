# Blueprint: The Attacker Snapshot — What a Hidden Ground Attacker Shows to the Player It Strikes (B-183)

This plan fixes **B-183** (`DESIGN.md` §4): "A ground attacker hidden by high ground or fog shows nothing to the struck
player; 5.0.14's damage `Visibility Snapshot` (`core` EffectData:62) shows it in fog and on the minimap, not as a
target." Plan [`2026-11-23-vision-and-detection`](2026-11-23-vision-and-detection.md) registered it and deferred it
as presentation (§2.2 rule 4, §14 F-2). That plan built the other two values of the same field: `Visible` (the
attack reveal of air-plane attackers, B-55) and `Hidden` (nothing). This plan builds the third and default value.

In StarCraft II 5.0.14, a weapon whose damage effect keeps the engine default `Visibility Snapshot` leaves the player
it strikes a **snapshot** of the attacker: a still, dimmed image of the attacker where it stood when it struck. The
snapshot is shown in the fog and on the minimap. It is never a target and never gives vision, and it stays until
that player sees the spot again or sees the attacker itself (§2).

The design in one paragraph: the simulation records, for each struck player, one **attack snapshot** per attacker
(the attacker's pose at the strike, in fixed point) whenever a `Snapshot` weapon strikes that player's unit from
where that player does not see it. It drops the snapshot once that player sees its cell or sees the attacker.
The records live in the simulation's unhashed presentation workspace. No tick decision reads them, so no checksum,
golden or schema moves. `PlayerView` publishes the viewer's own snapshots as ready render records, and the client
draws them from a small pool of bubble entities with a data-driven "snapshot" look. Those entities are not
`LiveBattleBubble`s, so picking, selection, the health bars and the command path never see them. The client has no
minimap (out of scope since Phase 5, Q0002). The minimap half of B-183 therefore moves to a new row, **B-204**,
which waits for a minimap; the record this plan publishes is that minimap's input (§14, draft question §15).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`8a15bff9` (checksum schema 45; the contact solver is about to take 46).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- New register rows are numbered **B-204** to **B-206** and the draft question **Q0112**: all provisional, the next
  free numbers at merge. Another plan has reserved B-202, B-203, Q0110 and Q0111.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S5 to S9 |
| Q0002: scoping is the agent's call if written down; remembered structures and a minimap out of scope since Phase 5 | Closed, standing | §14 (B-204, remembered structures) |
| Q0076: an undetected cloaked enemy shows only its shimmer | Closed | D-3 (a concealed attacker leaves no snapshot unless detected) |
| Q0101 4(b): an attack reveal reaches the struck player and its allies | Closed | D-2 (the snapshot reaches the same players) |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | Not touched: no movement, no outcome moves |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| **Draft, not filed:** "The attacker snapshot: the minimap, how long it stays, and concealed attackers" | Proposed for the orchestrator to file as Q0112 (§15); defaults followed meanwhile | D-2, D-3, D-4, B-204 |

---

## 1. Summary

| | Today (`8a15bff9`) | After this plan |
|---|---|---|
| A `Snapshot` weapon (every ground weapon, every armed structure) strikes p's unit from a cell p does not see | p sees nothing: no image, no marker | p sees a still, dimmed snapshot of the attacker where it stood at the strike. It is refreshed by every later strike and dropped once p sees that cell or sees the attacker itself |
| The snapshot as a target | — | Never: it is not a `LiveBattleBubble`, so no pick, selection, inspection, health bar or targeted order reaches it. A right-click on it is a point order, as on bare ground. The simulation's targeting is unchanged |
| A concealed attacker (cloaked or burrowed) | Nothing | A snapshot only if p detects the attacker's cell at the strike (default, **verify**, §15 part 3) |
| An air-plane attacker (`Visible`, B-55) | Revealed for 54 ticks | Unchanged: no snapshot; the reveal shows the live unit |
| Minimap | No minimap in the client | Still none; the minimap half becomes **B-204**, deferred, and `PlayerView::attack_snapshots` is its input (§15 part 1) |
| Hashed state, checksum, schema | — | **Unchanged**: the records sit in the unhashed `PresentationWorkspace`, and no tick decision reads them. No schema number is needed, and no golden or measured world moves |
| Size | — | M: five packages (WP0 to WP4), one S and two M of code |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`; `core EffectData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CEffectDamage` default, `core EffectData:61–69`: `Visibility Snapshot` (:62) | Every damage effect without an override snapshots its caster [DATA] |
| S3 | The `Visibility` survey of every layer (51 `CEffectDamage` and 6 `CEffectLaunchMissile` entries; plan `2026-11-23-vision-and-detection` §2.3, S6, S7): every damage effect of an air-plane attacker is `Visible`, three are `Hidden` (`PsiStormDamage`, `liberty EffectData:2099`; `90mmCannonsDummy`, `voidmulti :1195`; `KillRemove`, `swarm :1296`), and every other weapon damage keeps `Snapshot` | Which shipped weapons snapshot [DATA]; ours carry it as `WeaponSpec::attack_visibility` (§3 item 1) |
| S4 | `CEffectLaunchMissile` default, `core EffectData:95–109`: `Visibility Snapshot` (:102). Only five spell launches override it, all to `Visible` (`CorruptionLaunchMissile`, `liberty :672`; `FrenzyLaunchMissile`, `:2946`; `ContaminateLaunchMissile`, `:2976`; `EyeStalk`, `swarm :2632`; `Doom`, `:2642`) | A weapon missile's launch also snapshots its caster, before its impact does [DATA]; B-205 (§14) |
| S5 | A Battle.net forum answer by a long-time map-data author (us.forums.blizzard.com/en/sc2/t/missile-turrets-can-spot-high-ground-units/26375, fetched 2026-11-24): "Ground units do not become targetable, they only appear in fog and on minimap." The same post gives the air units' 3 game seconds and gives **no** duration for ground units | `Snapshot`: shown in fog and on the minimap, never targetable [EXT] |
| S6 | SC2Mapster, *How to prevent Siege Tank in fog of war* (sc2mapster.com/forums/development/data/162461; search abstract, the page now redirects to CurseForge): a tank that fires from fog "flashes into view for a brief moment"; "a sieged tank looks dimmed through the fog but still can't be targeted", and the image gives no vision | The snapshot is a dimmed image, not vision and not a target [EXT]. "A brief moment" describes the flash of the attack; the dimmed image is what stays (S7) |
| S7 | SC2Mapster data forum threads on fog snapshots (search abstracts, 2026-11-24): a unit's snapshot "persists until the unit is revealed in sight again, or is discovered to be dead"; "the snapshot is an actor that is independent of the unit" | How long a snapshot lasts: until the player sees the unit, or learns it is gone by seeing its spot [EXT, informal, **verify**] |
| S8 | `CUnit` `FogVisibility` (S1): `Snapshot` on the mineral fields and the Zerg crawlers and dens (`liberty UnitData:52`, `:388`, …), `Dimmed` on zones and bridges, and no `FogVisibility` on the Siege Tank (sieged or not) or any other army unit | The dimmed tank in the fog of S6 comes from its damage effect's snapshot (S2), not from a fog setting of the unit [DATA + INF] |
| S9 | `CUnit` default `DeathRevealType Snapshot`, `DeathRevealDuration 4`, `DeathRevealRadius 2` (`core UnitData:26–29`) | The engine's other snapshot reveal, with a duration of its own. The attack snapshot has no duration field anywhere in S1 [DATA]. This supports S7: the attack snapshot ends by a rule, not a timer [INF] |
| S10 | Liquipedia, *High Ground and Low Ground* (search abstract; the page refused fetching, HTTP 429): "Units on the high ground that attack will reveal their position (but not necessarily grant vision)" | High ground or fog: the same rule [EXT] |

### 2.2 The rules that follow

1. **Who leaves a snapshot.** A unit whose weapon damage keeps `Visibility Snapshot` (S2, S3), when that damage
   lands on a unit of another player. In 5.0.14 that is every ground weapon and every armed structure. An air-plane
   attacker (`Visible`) is revealed live instead (B-55). A `Hidden` damage effect shows nothing. [DATA]
2. **Who sees it.** The owner of each struck unit, as for the reveal (`Visibility` is applied per struck unit; Q0101
   4(b) adds allies, and the simulation has no teams). An area strike leaves a snapshot for every struck unit's
   owner. [DATA + INF]
3. **What it shows.** The attacker as it stood when the damage landed, dimmed (S6), still, in the fog and on the
   minimap (S5). It gives no vision of anything around it and is never a target (S5, S6). [EXT]
4. **When it is needed.** Only when the struck player does not already see the attacker. A player who sees the
   attacker sees the unit itself. [INF]
5. **High ground or fog.** No difference: in both, the attacker stands in a cell the struck player does not see
   (S10). The rule never asks why. [EXT + INF]
6. **How long it stays.** Until the struck player sees the snapshot's spot, or sees the attacker (S7). A later
   strike replaces it with the attacker's new pose. The attacker's death out of sight does not remove it (S7: "or is
   discovered to be dead", which needs sight of the spot). No duration exists in the data (S9). [EXT, **verify**,
   §15 part 2]
7. **Cloaked and burrowed attackers.** S1 to S10 are silent. A snapshot that showed an undetected cloaked or
   burrowed attacker would show more than the struck player may see of it in plain sight (Q0076: only a shimmer
   for a cloaked unit, nothing for a burrowed one). The default is that a concealed attacker leaves a snapshot only
   where the struck player detects it at the strike. Detection is a plain circle that needs no sight (Q0101 2(b)).
   [INF, **verify**, §15 part 3]
8. **The launch.** A weapon missile's launch also snapshots its caster (S4). That snapshot comes a flight time
   earlier than the impact's, from the launch position. Ours records the impact only (B-205, §14). [DATA]

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| Ground weapons default to `Snapshot`; air-plane weapons are `Visible` | High | S2, S3 [DATA] |
| The snapshot is never a target and gives no vision | High | S5, S6 [EXT, two independent authors] and the field's meaning beside `Visible` |
| It is shown in the fog and on the minimap | High | S5 [EXT] |
| It is dimmed and still | Medium | S6 ("looks dimmed"); "still" follows from a snapshot being an image held apart from the unit (S7) [EXT + INF] |
| It stays until the spot or the attacker is seen again | Medium-low, **verify** | S7 (informal), S9 (no duration field) |
| High ground and fog behave alike | Medium | S10, and the rule never names the cause |
| A concealed attacker leaves a snapshot only where detected | Low, **verify** | No source; chosen so that nothing hidden leaks (Q0076) |
| The launch of a missile also snapshots its caster | High for the data, effect unmeasured | S4 [DATA] |

---

## 3. Diagnosis (on `8a15bff9`, read from the code)

1. **The data is already there.** `sim_types::AttackVisibility` (`crates/sim_types/src/weapon.rs`) has `Snapshot`
   (the serde default), `Visible { reveal_ticks }` and `Hidden`, and `WeaponSpec::attack_visibility` carries it.
   The ten air-plane weapons author `Visible(reveal_ticks: 54)` in `assets/catalog/units.ron`. Every other weapon
   takes the default `Snapshot`. The doc of `AttackVisibility::Snapshot` says "The simulation records nothing; the
   presentation is bug B-183."
2. **One hook already sees every strike.** `SimWorld::record_attack_reveal` (`crates/sim_core/src/combat_resolution.rs`)
   runs for a direct hit (`apply_weapon_hit`) and for each unit an area strikes (`strike_area`). It returns early
   for every weapon that is not `Visible`. Nothing is recorded for `Snapshot`.
3. **Unhashed presentation state has a home.** `PresentationWorkspace` (`crates/sim_core/src/world/presentation.rs`)
   is held by `SimWorld::presentation`. Its documented invariant is "nothing here is hashed, consumes randomness or
   is read by a tick decision, so recording or dropping it cannot move a checksum". It already keeps state across
   ticks (projectile sources), in fixed point.
4. **The view is written for the presented tick only.** `refresh_hud_model` (`crates/client/src/battle_hud/model.rs`)
   writes `PlayerView` when the presented tick changes, so a frame that executes several ticks writes one view.
   Anything recorded per tick and published only on that tick would be lost. The snapshot must be state that lasts
   until its end rule (rule 6), not an event.
5. **`PlayerView` is the one viewer-scoped output** (`crates/sim_core/src/player_view.rs` module doc): "fog policy
   ... live[s] here, where a headless test can reach [it]". `attack_revealed` (B-55) is its model: a sorted list
   that is refilled without allocating.
6. **The client's one fog rule** is `BattleFog` (`crates/client/src/battle_pipeline/fog.rs`). It copies the view's
   lists into retained buffers. `apply_battle_fog` (`crates/client/src/battle_hud/fog.rs`) hides
   `LiveBattleBubble`s by `Presence`. Picking (`battle_input/mod.rs`), the stance and effect systems
   (`effects/stance.rs`, `effects/systems.rs`) and the health bars all query `With<LiveBattleBubble>`. An entity
   without that marker can be drawn and still never be picked or targeted.
7. **The smog reads every `BubbleInstance`.** The source query in `crates/client/src/smog/mod.rs` filters only
   `Without<SmogVolumeMarker>`, so a new bubble entity would push the smog unless it is excluded.
8. **A look can be data.** `assets/catalog/effects.ron` `defaults.concealment` names held looks (`cloak_own`,
   `cloak_shimmer`, `detected_mark`) whose `Shell` block modulates a bubble's own material (`ShellLook`: tint, rim,
   veil, matte, shadow; `crates/client/src/effects/catalog/mod.rs`).
9. **Still frames exist.** `FlipbookLanes::once(span, started_at, hold_last)` (`crates/client/src/flipbook_clock.rs`)
   over a one-layer `FlipbookSpan` holds one layer for good.
10. **No minimap, no remembered structures.** The client draws neither. Phase 5 put both out of scope
    (`2026-11-02-phase-5-playable-client.md` §6), confirmed by Q0002 item 1. Its decision D-10 is that an enemy
    structure disappears with sight.

---

## 4. Design decisions

### D-1. The snapshot is unhashed simulation presentation state, published per viewer

Three places could hold it:

| Option | Correct per player | Survives a frame of several ticks | Checksum | Verdict |
|---|---|---|---|---|
| (a) Hashed `Entity` field, like `attack_reveal_ticks` | Yes | Yes | Moves every golden, needs a schema (47) | Rejected: no tick decision reads it, so hashing it only costs |
| (b) Client-side, from per-tick strike events | Only if no tick is skipped | No (§3 item 4) | None | Rejected: it loses snapshots and puts fog policy in the client |
| (c) The simulation's `PresentationWorkspace`, published in `PlayerView` | Yes: written by the one viewer-scoped writer | Yes: state with an end rule | **None** | **Chosen** |

(c) keeps the policy where `PlayerView`'s module doc puts it, testable headless, and costs no schema. The records are
deterministic (fixed point, sorted, no randomness), but they are not hashed. The workspace's invariant forbids any
tick decision to read them. WP1 proves that with a unit test (P-9) and with every golden and sandbox checksum
byte-identical on the four builds (§5).

### D-2. The rule: record, refresh, drop

**Record** (`combat_resolution.rs`, beside `record_attack_reveal`, called from the same two places, which a new
`record_attack_presentation(source, victim_id)` wraps so the two cannot drift apart). A strike of `source` lands on
unit u owned by p. A snapshot of the attacker a is recorded for viewer p when **all** of these hold:

```text
source.weapon.attack_visibility == Snapshot
&& p != source.owner                                   # never one's own player
&& a exists, is alive and is not hidden                # a dead attacker (a Baneling) leaves nothing
&& !shown_to(p, a)                                      # p does not see a already (rule 4)
&& (!a.is_concealed() || detected_by(p, a.position))   # rule 7, D-3
```

`shown_to(p, a)` is the simulation's own rule for "p's interface shows a", the rule `BattleFog::presence` copies.
It holds when `sees_enemy(p, a)` holds (a `Visible` cell, or an attack reveal) and a is either unconcealed or
`is_cloak_revealed` for p. It is one new `pub(crate)` function in `target_acquisition.rs`, next to `sees_enemy`. It
is the D-2 record rule and the D-2 drop rule, and nothing else reads it.

As for the reveal, a strike that barriers, shields or armour reduce to zero still counts, and a strike skipped
because its target was unavailable does not. The record goes to the struck unit's owner and that player's allies
(Q0101 4(b)). The simulation has no teams, so that is the owner alone (as `record_attack_reveal` documents).

**Refresh.** At most one record exists per (viewer, attacker). A new strike overwrites its pose and its `struck`
tick. If a has moved, the snapshot moves with the latest strike (rule 6).

**Drop** (`world/vision.rs`, a new `prune_attack_snapshots` at the end of `update_vision`, after the vision maps of
this tick are final). A record for viewer p of attacker a is removed when either holds:
- p's vision map has the record's cell `Visible` (p sees the spot), or
- a exists, is alive, is not hidden, and `shown_to(p, a)` (p sees the attacker itself).

Nothing else removes it. In particular, a's death out of p's sight does not (rule 6). A record whose attacker has
died is kept with its own pose and dropped by the first rule.

**Phase order.** Vision (phase 3, prune) comes before combat (phase 7, record), and the view is written after the
tick. A strike recorded in phase 7 of tick t is therefore listed in the view of tick t. It is pruned in phase 3 of a
later tick, from vision computed in that tick. A record is never created for an attacker that phase 3 of the same
tick showed to p, because `shown_to` reads the same vision maps.

### D-3. Concealed attackers

A concealed attacker (`Entity::is_concealed`, cloaked or burrowed and not revealed by a status) leaves a snapshot for
p only when p detects the attacker's cell at the strike (`PlayerVisionMap::is_detected`). Detection is a plain
circle (Q0101 2(b)), so p may detect a cell it does not see. The snapshot then shows the unit as detected units are
shown: plainly, with the snapshot look and without a shimmer. An undetected concealed attacker leaves nothing, so no
snapshot ever shows a unit the viewer could not see in plain sight. This is a **verify** default (§15 part 3). It
cannot leak: detection of the cell is the viewer's own knowledge.

### D-4. The record and what `PlayerView` publishes

**Record** (`world/presentation.rs`, fixed point; C7 allowances unchanged):

```rust
/// One player's snapshot of an attacker that struck it unseen (plan `2026-11-24-attacker-snapshot` D-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttackSnapshotRecord {
    pub(crate) viewer: PlayerId,
    pub(crate) attacker: EntityId,
    /// The tick of the strike that last refreshed it.
    pub(crate) struck: Tick,
    pub(crate) pose: SnapshotPose,
}

/// What the snapshot shows of the attacker, as it stood at the strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotPose {
    pub(crate) unit_type: UnitTypeId,
    pub(crate) owner: PlayerId,
    pub(crate) position: SimVec2,          // world units
    pub(crate) heading: SimAngle,          // binary angle
    pub(crate) turret: SimAngle,           // binary angle
    pub(crate) mode: Option<u8>,           // index into the archetype's modes, settled
    pub(crate) hp_fraction: SimScalar,     // 0..=1
    pub(crate) shield_fraction: SimScalar, // 0..=1
}
```

`PresentationWorkspace` gains `attack_snapshots: Vec<AttackSnapshotRecord>`, sorted by `(viewer slot, attacker)`,
with `record_attack_snapshot`, `prune_attack_snapshots(keep: impl FnMut(&AttackSnapshotRecord) -> bool)` and an
iterator over one viewer's records. Insertion is a binary search, and the vector keeps its capacity. A
`// PERFORMANCE RATIONALE:` block states why it is not a map: ordered and allocation-free at its high-water mark,
with O(log n) insertion and one O(n) prune pass per tick, where n is at most players × attackers.

**Published** (`crates/sim_types/src/player_view.rs`):

```rust
/// An attacker that struck the viewer's units from where the viewer did not see it, shown as it stood at its
/// last such strike: a still image, never a target (`StarCraft` II's damage `Visibility Snapshot`; plan
/// `2026-11-24-attacker-snapshot` D-4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttackSnapshotView {
    /// The attacker. It may since have moved or died; the record says only what the viewer saw.
    pub entity: EntityId,
    /// The tick of the strike the image is from.
    pub struck: Tick,
    /// The image: the attacker's render record at that strike, still (zero velocity, idle, unselected,
    /// unconcealed, not housed, no hit flashes).
    pub bubble: BubbleRenderInstance,
}
```

`PlayerView` gains `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub attack_snapshots:
Vec<AttackSnapshotView>`, sorted by `entity`, so records without snapshots keep their bytes. `PlayerView::clear`
clears it. `write_player_view` fills it from the viewer's records. Each bubble is built by a new
`snapshot_bubble(pose, arch, map)` in `world/snapshot.rs`, which shares with `bubble_instance` the parts that
depend only on the archetype, the mode and the position: altitude, shape, silhouette, drawn radius, the weapon visual
of the mode, and the settled `StanceView` of the mode. So a snapshot looks like its attacker did. The refactor must
leave every live `BubbleRenderInstance` byte-identical; `writing_a_snapshot_changes_no_checksum` and the existing
render snapshot tests guard that. The archetype is looked up by `unit_type` in the catalog, so a dead attacker's
snapshot is still built. Filling reuses each element's storage (`clone_from`), so a warmed view allocates nothing
unless the list grows.

The doc of `AttackVisibility::Snapshot` becomes: "The struck player keeps a still image of the attacker where it
struck from, never a target, until it sees that spot or the attacker (plan `2026-11-24-attacker-snapshot` D-2)."

### D-5. The client: drawn, never picked

- **`BattleFog`** copies `view.attack_snapshots` into a retained `Vec<AttackSnapshotView>`, element by element with
  `clone_from`, and exposes it as `attack_snapshots(&self) -> &[AttackSnapshotView]`. `retained_buffers` and
  `buffers` list it as `fog.attack_snapshots`. `close` clears it. The module doc gains a paragraph: a snapshot is
  an image the view lists, not a unit, and nothing reads it but the snapshot pool. The fog stays the only place the
  rule is copied, and it knows nothing its view did not say.
- **Pool** (new `crates/client/src/battle_hud/attack_snapshots.rs`, registered under `BattleProfile::Player` only,
  in `PreparePresentation` after `refresh_battle_fog`):
  - `AttackSnapshotPool` holds the spawned entities. Each carries the marker component `AttackSnapshotBubble`, a
    `BubbleInstance`, a `Transform`, a `Mesh3d` (sphere or dome by shape) and its own `BubbleDomeMaterial`. It has
    **no** `LiveBattleBubble`, no selection ring, no health bar and no indicator children.
  - Every frame on which the fog was refilled, entry i of the list is shown on pooled entity i. The entity is
    restyled only when its `(entity, struck)` differs from what it shows: instance, transform, mesh and material
    (the unit type's flipbook texture and uniform, through the same `sync_bubble_material` the live sync uses).
    Entities past the list's end are set `Visibility::Hidden`. The pool grows by spawning only when the list is
    longer than any list before it, so the steady state spawns nothing and allocates nothing.
  - **Still image:** the material's lanes are `FlipbookLanes::once` over a one-layer span, the first layer of the
    shown stance's idle clip, held. The clock moves, and the image does not.
  - **Look (data):** `assets/catalog/effects.ron` `defaults` gains `snapshot: "attack_snapshot"`, a new effect with
    one held `Shell` layer: a cool grey tint over most of the media, a partial veil, the specular glare matted and the
    shadow taken away, so it reads as "seen in the fog, not here now" (S6's "dimmed"). The pool resolves the named
    effect once when the catalog loads and writes its `ShellLook` into each pooled material's modulation. It spawns
    no effect instance. The catalog's validation refuses a `snapshot` default that is not exactly one held `Shell`
    layer, with an error naming the file and the effect. The colours are an art-direction choice for review (WP3);
    no number is fixed in Rust.
  - **Media:** the snapshot draws its unit type's real flipbook from `FlipbookRegistry`. A unit type whose media is
    unavailable gets the typed unavailable status the live sync already reports; nothing is substituted (charter
    §1.2).
- **Never a target.** The pool's entities are absent from every `With<LiveBattleBubble>` query, so picking, box
  selection, inspection, the selection panel, health bars, the stance and effect systems and the order card never
  meet them. A right-click on a snapshot finds no bubble and issues the point order a click on bare ground issues.
  In the simulation, an `AttackTarget` on the attacker is refused as before (the weapon filter needs sight).
- **Smog.** The smog source query gains `Without<AttackSnapshotBubble>`, so an image pushes no smog.
- **Sandbox, editor, headless check.** There is no `BattleFog`, so there is no pool and nothing changes; they keep
  showing the truth.

### D-6. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-183 restated with S2 to S9 ("… shows the struck player nothing; 5.0.14 leaves it a still, dimmed snapshot of the attacker in the fog and on the minimap, never a target, until it sees that spot or the attacker"); status "open; plan `2026-11-24-attacker-snapshot`". Register **B-204** (no minimap, so the snapshot's minimap half; deferred), **B-205** (the launch snapshot, S4; deferred), **B-206** (spell damage nodes carry no `Visibility`: a spell's caster is neither revealed nor snapshotted; deferred) | WP0 |
| `DESIGN.md` §4 | B-183 **fixed** for the battlefield view, with commits, tests and evidence; its minimap half pointed to B-204. The "Smog fog presentation" row gains "an attacker that struck the viewer unseen leaves a still snapshot (`PlayerView::attack_snapshots`), drawn and never picked". The "Combat" row names the snapshot beside the reveal | WP4 |
| `docs/plans/2026-11-23-vision-and-detection.md` §14 F-2 | "*Taken up by plan `2026-11-24-attacker-snapshot`.*" | WP4 |
| New `docs/evidence/2026-11-24-attacker-snapshot.md` | Baseline checksums, P-9's result, the four builds' golden checksums before and after, the capture of WP3 | WP0 creates; WP1 to WP4 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP4) | plan, WP1, WP4 |

---

## 5. Determinism and checksum

- **No hashed field, no encoding, no schema.** `checksum.rs` reads neither `PresentationWorkspace` nor `PlayerView`.
  `CHECKSUM_SCHEMA_VERSION` stays where `main` has it at merge (45, or 46 once the contact solver lands). The
  provisional 47 named in the brief is **not** needed.
- **No tick decision reads the records.** The workspace's invariant says so. P-9 tests it: two worlds run the same
  commands, one of them clearing every snapshot record after each tick through a `#[cfg(test)]` hook, and their
  checksums agree on every tick.
- **Fixed point and order.** Poses are `SimVec2`, `SimAngle` and `SimScalar`. The list is sorted by `(viewer slot,
  attacker)`. Floats appear only when `write_player_view` builds the `BubbleRenderInstance`, the presentation output
  C7 allows; the conversion calls are those `bubble_instance` already uses, so C7's allowance list should not grow
  (**verify** in WP1; if it does, the allowance is added with this plan's reference).
- **No randomness, clock or new collection type.** `Vec` with binary search; `PerPlayer` is not needed.
- **Proof on the four builds (WP1).** Before and after WP1, `golden_replay` (both fixtures) and
  `DEFAULT_SANDBOX_CHECKSUM` are byte-identical on native debug and release and on `wasm32-wasip1` debug and
  release (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`). Any move stops the package (§8).
- **Cost.** One extra branch per strike for a non-`Snapshot` weapon. For a `Snapshot` weapon, a `shown_to` check and
  an O(log n) insertion. One prune pass per tick over the records, each a cell lookup and at most one entity lookup.
  `cargo bench -p sim_core --bench throughput` runs in WP1's package gate; every scenario stays at or above real
  time (charter §3.1).

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a plateau with no ramp
in reach, so the low ground does not see up), with the catalog's shipped units from `sim_fixtures`.

- **P-1 (record):** a `Snapshot`-weapon attacker on the plateau strikes p's unit below. After that tick, p's
  `attack_snapshots` lists it once, at its position and heading at the strike, with `struck` equal to that tick.
- **P-2 (only the struck player):** the attacker's own player, and any third player whose units were not struck,
  list nothing of it. No player's `attack_revealed`, `revealed` or targetable set gains it.
- **P-3 (not when seen):** with p seeing the attacker's cell (a unit of p on the plateau), a strike records nothing.
- **P-4 (other visibilities):** a `Visible` weapon records no snapshot (it reveals, B-55), and a `Hidden` weapon
  records nothing (constructed from a catalog row with `attack_visibility: Hidden`).
- **P-5 (refresh):** a second strike after the attacker moved replaces the pose. There is one entry per attacker, and
  `struck` is the later tick.
- **P-6 (drop on sight of the spot):** a unit of p walks up until the snapshot's cell is `Visible`. On that tick the
  entry is gone. When p's unit walks away and the cell is fogged again, the entry does not come back.
- **P-7 (drop on sight of the attacker):** the attacker walks into p's sight elsewhere; the entry is gone.
- **P-8 (death out of sight):** the attacker dies where p does not see it. The entry stays until p sees the
  snapshot's cell.
- **P-9 (no checksum moves):** the paired-world test of §5; and every golden and the sandbox checksum are
  byte-identical on the four builds.
- **P-10 (never a target):** while the snapshot is listed, p's units neither acquire nor fire at the attacker. Its
  cell is not `Visible` and it is not attack-revealed, so `passes_weapon_filters` is false. An `AttackTarget` from
  p on it is refused or dropped exactly as it was before this plan.
- **P-11 (concealed, D-3):** an undetected cloaked attacker leaves no entry. The same attacker inside p's detection
  circle, but outside p's sight, leaves one.
- **P-12 (area):** an area strike that hits units of p leaves one entry for p (not one per struck unit).
- **P-13 (client, drawn):** under the `Player` profile, after such a strike, exactly one `AttackSnapshotBubble` is
  visible at the snapshot's position with the snapshot look's modulation. No `LiveBattleBubble` of the attacker is
  visible.
- **P-14 (client, never picked):** a click at the snapshot's screen position selects nothing. A right-click there
  with p's units selected submits a point order (`Move` or the attack-move point form), never `AttackTarget`.
- **P-15 (client, gone):** once the view no longer lists it, the pooled entity is hidden on that frame.
- **P-16 (client, steady state):** with snapshots held and the list unchanged, the fog refill, the pool and the live
  sync allocate nothing and fire no asset event. A refreshed snapshot of the same unit type spawns nothing.
- **P-17 (GPU, real shaders):** on the real bubble shader and the shipped media, the pixels at a snapshot's
  projected footprint differ from the same frame without it. Their mean chroma is lower than the same unit drawn live
  in the same light (the look is dimmed, as the data authors it).
- **P-18 (sandbox unchanged):** the sandbox profile draws no snapshot entity, and its frames are byte-identical to
  before.

---

## 7. Worlds that move

None is expected. No measured world, matrix battle, golden replay, sandbox checksum or showcase take reads a
snapshot. The showcase records without a player's fog (**verify** in WP0 that `client_diagnostics` uses no
`BattleProfile::Player`; if it does, the storyboard's frames with an unseen ground attacker are listed in WP0 and
re-recorded in WP3 with the reason). The render snapshot is unchanged byte for byte (D-4's refactor).
`PlayerView` gains a field that is skipped when empty, so serialized views without snapshots keep their bytes.

---

## 8. Stop rules

1. A golden, sandbox, `sim_map` determinism or matrix checksum moves in any package: stop, trace, report. The design
   says nothing hashed changed.
2. P-9 fails: a tick decision reads the records. Stop and report; do not hash them to make it pass.
3. The refactor of `bubble_instance` changes a live render record: stop; the refactor is wrong.
4. A snapshot entity is reachable by picking, selection or a targeted order: stop; D-5's isolation is broken.
5. The throughput bench drops below real time in any scenario: stop and report.
6. Anything under `crates/sim_core/src/movement/` or the contact solver would need a change: stop. This plan touches
   neither.

---

## 9. Client consequences

- New module `battle_hud/attack_snapshots.rs` and the `BattleFog` copy (D-5). The fog refill gains one list, and
  the pool gains one system.
- `effects.ron` gains one effect and one default. `effect_catalog_tests.rs` gains the default's coverage and its
  refusal (a `snapshot` default with a non-`Shell` or unheld layer).
- The smog source query excludes the snapshot marker (`smog_golden_tests.rs` re-run in the package gate; no golden
  is expected to move, because no smog golden holds a snapshot).
- GPU: a new `attack_snapshot_gpu_tests.rs`, which locks the GPU itself through `crates/client/tests/support/gpu_run.rs`
  (one child per test). `concealment_gpu_tests.rs` runs in the package gate on its catalog triggers.
- `crates/gate/suites.ron`: the new test file is catalogued (class `Gpu`, triggered by `battle_hud/`, `battle_pipeline/fog.rs`,
  `bubble_shader.rs`, `effects/catalog/` and `assets/catalog/effects.ron`). The new `sim_core` test file is
  catalogued with the existing vision tests' class. `sync_allocation_count_tests` gains `battle_hud/attack_snapshots.rs`
  among its triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (D-6) | WP0, WP4 |
| `docs/evidence/2026-11-24-attacker-snapshot.md` | New | WP0 to WP4 |
| `crates/sim_types/src/player_view.rs` | `AttackSnapshotView`, `PlayerView::attack_snapshots`, `clear` | WP1 |
| `crates/sim_types/src/weapon.rs` | `AttackVisibility::Snapshot` doc | WP1 |
| `crates/sim_types/src/lib.rs` | Export `AttackSnapshotView` | WP1 |
| `crates/sim_core/src/world/presentation.rs` | `AttackSnapshotRecord`, `SnapshotPose`, the workspace list and its methods; module doc invariant | WP1 |
| `crates/sim_core/src/combat_resolution.rs` | `record_attack_presentation` wrapping the reveal and the new snapshot record; module doc | WP1 |
| `crates/sim_core/src/target_acquisition.rs` | `shown_to` | WP1 |
| `crates/sim_core/src/world/vision.rs` | `prune_attack_snapshots` at the end of `update_vision`; module doc | WP1 |
| `crates/sim_core/src/world/snapshot.rs` | `snapshot_bubble` and the shared parts of `bubble_instance` | WP1 |
| `crates/sim_core/src/player_view.rs` | Fill `attack_snapshots`; module doc | WP1 |
| `crates/sim_core/tests/attack_snapshot_tests.rs` | New: P-1 to P-8, P-10 to P-12 through public APIs (`SimWorld`, `PlayerCommand`, `write_player_view`) | WP1 |
| `crates/sim_core/tests/player_view_tests.rs` | P-2's view lists; the warmed refill allocates nothing with snapshots | WP1 |
| `crates/client/src/battle_pipeline/fog.rs` | The copy, accessor, buffers, doc; unit tests | WP2 |
| `crates/client/src/battle_hud/attack_snapshots.rs` | New: pool, marker, still lanes, look | WP2 |
| `crates/client/src/battle_hud/mod.rs` | Register the pool under `Player` | WP2 |
| `crates/client/src/smog/mod.rs` | `Without<AttackSnapshotBubble>` | WP2 |
| `crates/client/src/effects/catalog/{schema,resolve,mod}.rs` | `defaults.snapshot`, validation | WP2 |
| `assets/catalog/effects.ron` | `attack_snapshot` and `defaults.snapshot` | WP2 (values reviewed in WP3) |
| `crates/client/tests/battle_fog_tests.rs` | P-13 to P-15 through a real session | WP2 |
| `crates/client/tests/effect_catalog_tests.rs` | The default and its refusal | WP2 |
| `crates/client/tests/sync_steady_state_tests.rs` | P-16: `fog.attack_snapshots` and the pool's capacity and entities stable; no asset event | WP2 |
| `crates/client/tests/sync_allocation_count_tests.rs` | P-16 counted: the live battle sync with snapshots held allocates nothing (valgrind) | WP2 |
| `crates/client/tests/attack_snapshot_gpu_tests.rs` | New: P-17, P-18 | WP3 |
| `crates/gate/suites.ron` | Catalogue the two new test files and the triggers (§9) | WP1, WP3 |
| `docs/plans/2026-11-23-vision-and-detection.md` | §14 F-2 pointer | WP4 |
| `docs/plans/README.md` | Row | plan, WP1, WP4 |

Not touched: `crates/sim_core/src/movement/`, the contact solver, `checksum.rs`, `units.ron`, `structures.ron`.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the code makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests the change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). GPU tests lock
the GPU themselves; `cargo test` is never wrapped in `with-gpu-lock`. Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/attacker-snapshot-plan`, and logs and captures go under
`~/scratch/attacker-snapshot/`.

### WP0: records and baseline (XS, docs only)

B-183 restated, B-204 to B-206 registered (D-6). The evidence file is created with the baseline: both goldens'
checkpoints and the sandbox checksum on native debug and release and on `wasm32-wasip1` debug and release. It also
records whether the showcase runs under a player's fog (§7). Commit `docs(design): restate B-183; register B-204 to
B-206 (plan 2026-11-24-attacker-snapshot WP0)`.

### WP1: the snapshot in the simulation and the view (M, `sim_types`, `sim_core`)

D-1 to D-4. Red first: `attack_snapshot_tests.rs` (P-1 to P-8, P-10 to P-12), the view tests (P-2), and unit tests
in `presentation.rs` (sorted insert, refresh, prune keep-function), in `combat_resolution.rs` (each condition of
D-2's record rule alone flips the outcome), in `vision.rs` (prune on the spot, on the attacker, not on death) and
in `snapshot.rs` (`snapshot_bubble` equals `bubble_instance` of an idle, unflashed, unselected entity in the same
pose; P-9's paired worlds). Then the code. Proof of §5 on the four builds, appended to the evidence. Throughput bench
in the package gate. **Acceptance:** P-1 to P-12 pass; §8 rules 1 to 3, 5 and 6 hold. Commit `feat(sim_core): a
hidden attacker leaves the player it strikes a snapshot (B-183)`.

### WP2: the client draws it, never picks it (M, `client`, data)

D-5. Red first: the fog unit tests (copy, sort, buffers, close), `battle_fog_tests.rs` P-13 to P-15 (a real skirmish
session; an enemy ranged ground unit placed beyond the viewer's sight and within its weapon range, or on a higher cliff
level of the skirmish map, strikes a unit of the viewer), the catalog tests, P-16 in `sync_steady_state_tests.rs`
and in `sync_allocation_count_tests.rs`. Then the code and the look's first values. **Acceptance:** P-13 to P-16
pass; §8 rule 4 holds; `smog_golden_tests.rs` and `concealment_gpu_tests.rs` unchanged. Commit `feat(client):
draw the attacker snapshot in the fog, never as a target (B-183)`.

### WP3: real-shader verification and the look's review (S, `client`)

`attack_snapshot_gpu_tests.rs` (P-17, P-18) on the radv GPU through the harness, and the same on lavapipe as CI runs
it. One capture of the skirmish scene with a snapshot beside a live unit of the same type, taken on the physical GPU
under `with-gpu-lock`, saved to `~/scratch/attacker-snapshot/`, and linked from the evidence. The orchestrator or the
owner reviews the look. Any change of the `attack_snapshot` values follows the review in this package. Commit
`test(client): the attacker snapshot on real shaders`.

### WP4: bookkeeping (XS)

D-6's WP4 rows. B-183 fixed for the battlefield view, and its minimap half pointed to B-204. The vision plan's F-2
pointer and the README row `Done`. `## As built` appended here. Commit `docs: close B-183 (plan
2026-11-24-attacker-snapshot WP4)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_core` `world/presentation.rs`, `combat_resolution.rs`, `world/vision.rs`, `world/snapshot.rs` | The record's order and reuse; each condition of the record rule; the prune rule; the pose bubble; P-9 |
| Integration | `crates/sim_core/tests/attack_snapshot_tests.rs` (new) | P-1 to P-8, P-10 to P-12 through `SimWorld`, `PlayerCommand` and `write_player_view` |
| Integration | `crates/sim_core/tests/player_view_tests.rs` | P-2; a warmed view with snapshots refills without allocating |
| Determinism | `golden_replay_tests.rs`, sandbox checksum, `sim_map` determinism, on four builds | §5: byte-identical |
| Unit | `client` `battle_pipeline/fog.rs` | The copy, accessor and buffers |
| Integration | `crates/client/tests/battle_fog_tests.rs` | P-13 to P-15 in a real session |
| Integration | `crates/client/tests/effect_catalog_tests.rs` | The shipped default and its refusal |
| Steady state | `crates/client/tests/sync_steady_state_tests.rs` | P-16: buffers, entities, no asset events |
| Allocation count | `crates/client/tests/sync_allocation_count_tests.rs` (valgrind) | P-16 counted for the live battle sync with snapshots held |
| GPU | `crates/client/tests/attack_snapshot_gpu_tests.rs` (new) | P-17, P-18 on real shaders and shipped media |
| Bench | `cargo bench -p sim_core --bench throughput` | Every scenario at least real time |

---

## 13. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1: a tick decision comes to read the records later | Hidden nondeterminism between checksum and behaviour | Invariant in the workspace's module doc; P-9 stays in the suite |
| R-2: records of dead attackers pile up in a long match | Memory growth | At most one per (viewer, attacker), and dropped when the spot is seen. The bound is players × attackers ever spawned, as small as the entity table's history. The evidence records the high-water mark of the longest shipped scenario |
| R-3: the snapshot leaks hidden information | A player learns of a unit it may not see | D-2 and D-3 read only the struck player's own vision and detection. P-2 and P-11 test it. The view writer is the only output |
| R-4: the "until seen" rule is wrong for 5.0.14 | A snapshot lingers longer than in the game | §15 part 2 asks the owner. Switching to a duration is a one-function change in the prune rule, with no checksum effect |
| R-5: the look reads as a live unit | The player shoots at nothing | WP3 review; the look is data; P-17 checks it is dimmed |
| R-6: the pool's material churn moves the steady-state tests | A red gate | Restyle only on `(entity, struck)` change; P-16 |
| R-7: a snapshot overlaps the live attacker on the same frame | Two images of one unit | D-2's drop rule removes the record on the tick the attacker is shown, and the view and the fog are written from the same tick |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-204 (new): the minimap half of B-183.** The client has no minimap (Phase 5 §6, Q0002 item 1). When one is
  built, it draws `PlayerView::attack_snapshots` as the attacker owner's marker at each `bubble.world_pos`. That is
  the reason the view, and not the client, holds the rule. Deferred (§15 part 1).
- **B-205 (new): the launch snapshot.** A weapon missile's launch also snapshots its caster (S4), a flight time
  before the impact's snapshot and from the launch position. It differs from ours only in when the image first
  appears, and for a missile that never lands (a lost target, B-54). Modelling it needs the launch's own visibility
  as data (`WeaponSpec` is hashed with every projectile, so a schema change) for a difference of a few ticks. Deferred. *Taken up by plan `2026-11-27-launch-and-spell-visibility`.*
- **B-206 (new): spell damage carries no `Visibility`.** `sim_types::effects::DamageDef` has no visibility field,
  so a spell's caster is neither revealed nor snapshotted to the player its damage strikes (5.0.14: `Snapshot` by
  default, `Hidden` for Psionic Storm, S3). Deferred to the effect engine's next damage change; its hook is D-2's
  `record_attack_presentation`, called with the effect's visibility. *Taken up by plan `2026-11-27-launch-and-spell-visibility`.*
- **Remembered structures** (`FogVisibility Snapshot` on structures, S8): out of scope since Phase 5 (D-10, Q0002
  item 1), and not registered as a bug by that decision. This plan's record and pool are a natural basis for them.
- **Snapshot of an attack-revealed air attacker after its reveal ends:** not in the data. A `Visible` effect does not
  snapshot. Not built.
- **Allies:** the simulation has no teams; D-2 follows Q0101 4(b), so a team model gives allies the record for free.

---

## 15. Draft owner question (filed as [Q0112](../questions/closed/0112-attacker-snapshot.md))

````markdown
# Q0112: The attacker snapshot: the minimap, how long it stays, and concealed attackers

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** 1(a), 2(a), 3(a), as built by plan `2026-11-24-attacker-snapshot`

## Question

In StarCraft II, a ground unit (or an armed structure) that shoots your units from somewhere you cannot see, such as
high ground or fog, leaves you a *snapshot*: a dimmed, still image of the attacker where it fired from. You cannot
target it and it gives no vision. Air units and the Colossus are different: they become fully visible and targetable
for 3 seconds (already built). Plan `2026-11-24-attacker-snapshot` builds the snapshot for the battlefield view (bug
B-183). Three details are not settled by the game data or by any source I found.

**1. The minimap.** The game also shows the snapshot on the minimap. Space Scuffle has no minimap: it was left out
in Phase 5 (Q0002).
- (a) Close B-183 for the battlefield view and keep the minimap half as a new bug, B-204, until a minimap is built.
  The simulation already publishes everything a minimap would draw. **Recommended.**
- (b) Build a minimap now, as its own plan, and draw the snapshot on it.

**2. How long the snapshot stays.** No duration is in the game data. Map-editor forum posts say a snapshot stays
until you see that spot again, or see the attacker itself.
- (a) Until you see the spot or the attacker; a later shot from the same attacker moves it. **Recommended**; it
  matches those posts.
- (b) A fixed time after each shot (for example 3 seconds, like the air units' reveal), and then it fades.
- (c) Like (a), but at most a fixed time.

**3. Cloaked or burrowed attackers.** No source says whether a cloaked or burrowed attacker leaves a snapshot.
- (a) Only if you detect the spot it fires from (with a detector in range). Otherwise you see nothing, as you
  would see nothing of it in plain sight. **Recommended**; it reveals nothing hidden.
- (b) Always, as for any other attacker.
- (c) Never.

Every option changes presentation only: no battle outcome, replay or checksum moves.

## Answer

````

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/` (`5444e12f13`), read with `grep` and `awk`: every `<Visibility` line of
  every layer's `EffectData.xml` mapped to its enclosing effect type (51 `CEffectDamage`, 6 `CEffectLaunchMissile`),
  every `FogVisibility` and `DeathReveal*` line of every `UnitData.xml`.
- Outside sources (Q0008), 2026-11-24: S5 fetched; S6, S7 and S10 from search abstracts (SC2Mapster now redirects
  to CurseForge, and Liquipedia answered HTTP 429). The Hive Workshop's damage-effect page answered HTTP 502.
- No prototype: WP1's record and drop rules are small and fully covered by constructed tests, and no world is
  expected to move.

---

## Amendment 1 (2026-11-25, orchestrator): Q0112 answered 2(c), a 1-second cap

The owner answered Q0112 1(a), **2(c)** and 3(a). Parts 1 and 3 are what this plan built. Part 2 changes D-3's
lifetime: a snapshot still ends when its struck player sees its cell or the attacker, and it now also ends **at most
1 second after the strike that made or last refreshed it** (a later strike by the same attacker on that player moves
the record and restarts the cap). The owner wants the value easy to tune, so it is data, not a code constant:

- **What changes.** The record keeps the tick of its last strike (unhashed, in `PresentationWorkspace`, like the pose).
  `prune_attack_snapshots` (`world/vision.rs`) also drops a record once `tick - strike_tick` reaches the cap. The cap
  is read at run time from the catalog (one authored field in seconds or ticks, beside the other vision or
  presentation rules the catalog already loads; 1 second is 25 ticks, `TICKS_PER_SECOND`), validated (a cap of 0 is
  refused with a typed error, since it would drop every snapshot before it is shown), and documented where it is
  authored.
- **Why.** Owner answer Q0112 2(c): a mobile high-ground attacker must not leave stale ghosts behind.
- **Tests (red first).** In `attack_snapshot_tests.rs`: a snapshot left alone disappears exactly when the cap is
  reached and not a tick earlier; a second strike before the cap moves the record and restarts the cap; the existing
  "seen cell" and "seen attacker" drops still end it earlier; the cap is read from the catalog (a scratch catalog with a
  different cap changes the lifetime); the validation refuses 0. The client steady-state and allocation tests keep
  passing (the record grows by one integer, no heap data).
- **Unchanged.** No checksum, golden or schema value moves (the records are unhashed and read by no tick decision);
  the proof on four builds of the plan's WP1 is repeated on the final tree. Q0112 moves to `closed/` with a
  `## Resolution` in the same branch.

## As built

Built on branch `attacker-snapshot-plan` from `b010d0a6` (main `8a15bff9`, schema 45), 2026-11-24, with Q0112's
defaults 1(a), 2(a), 3(a). Evidence: [`docs/evidence/2026-11-24-attacker-snapshot.md`](../evidence/2026-11-24-attacker-snapshot.md).

| WP | Commit | What |
|---|---|---|
| WP0 | `7ba9581d` | B-183 restated; B-204 to B-206 registered (provisional numbers); baseline on four builds |
| WP1 | `f547d7e2` | D-1 to D-4 in `sim_types` and `sim_core`; P-1 to P-12 |
| WP2 | `032cf676` | D-5 in the client; P-13 to P-16 |
| WP3 | `3e8b5295` | P-17, P-18 on radv and lavapipe; capture for review |
| WP4 | this commit | Records |

No checksum, golden, schema or measured world moved: both goldens and `DEFAULT_SANDBOX_CHECKSUM` pass unchanged on
native debug and release and `wasm32-wasip1` release and debug, on the WP1 commit and on the final tree.

Deviations from the plan:

1. **The view leaves out a shown attacker's snapshot (R-7).** P-7 found that an attacker walking into the viewer's
   sight during a tick's movement phase, after that tick's vision phase pruned, was listed both as a unit in sight and
   as a snapshot for one tick. `write_player_view` now skips a record whose attacker it shows now
   (`SimWorld::snapshot_attacker_shown`, the second half of D-2's drop rule, shared with the prune). The record itself
   is still dropped by the vision phase on the next tick. Presentation only.
2. **The pose carries no weapon visual.** `snapshot_bubble` derives the visual from the pose's settled mode
   (`ground_weapon_override` of `arch.modes[mode]`), as `bubble_instance` derives it from the entity's active
   siege weapon. They agree for every `SwitchMode` mode; they would differ only for a `SiegeModeToggle` stance whose
   mode the archetype does not declare, which no shipped unit has.
3. **Shared parts of the two render records** are a private `Placed` (world position, ground height, drawn radius,
   weapon visual) and `published_share`/`fraction`; C7's allowance of `world/snapshot.rs` rose from 3 to 7, with
   this plan's reference.
4. **`PlayerView::attack_snapshots` is filled by `clear` then `push`**, not element-wise `clone_from`:
   `AttackSnapshotView` holds no heap storage, so the refill allocates nothing once warm (P-2's test). The client's
   copy uses `Vec::clone_from`, which reuses each element.
5. **A shared test support file**, `crates/client/tests/support/attack_snapshot_stage.rs` (not in §10), stages the
   fight for `battle_fog_tests`, `sync_steady_state_tests`, `sync_allocation_count_tests` and the GPU test: it reads
   the shipped skirmish map's cliff levels for the edge farthest from the local units, with two cells of open ground
   round each soldier (a soldier at a cliff's foot stands outside its terrain-radius movement domain).
6. **Also touched:** `crates/client/tests/system_ownership_tests.rs` (the pool is a declared writer of bubble
   materials), `crates/client/tests/support/battle_harness.rs` (`unit_on_screen` finds a unit's own bubble, never its
   image), `effects/catalog/error.rs` (`SnapshotLookNotHeldShell`). `EffectCatalog::snapshot_look` reads the look.
   A missing `defaults.snapshot` draws the image without a look, as every other optional default; the shipped file
   names one and a test requires it.
7. **P-16's allocation count** stages the snapshot before the session begins and makes the struck unit
   invulnerable, so the running children hold one unchanged image through the window; the view writer and the fog
   refill run in the running live child beside the pool.
8. **P-17's dimming bound** is that the image keeps at most half the live unit's mean chroma: "lower" alone also
   passed without the look (the still frame is a little duller, 0.80), so the bound tests the look.
9. **P-18's "frames byte-identical"** is shown structurally (no pool, no system, no entity under the sandbox),
   not by a pixel comparison across trees.
10. **The showcase** (§7) runs under the player's fog. Its `battle` take shows one snapshot on ticks 313 to 328 and
    345 to 348 (the opening of the lane fight); the `effects` take none. CI renders the video, so nothing was
    re-recorded. Measured after WP2, since no earlier tree records snapshots.
11. **Not run here:** `cargo gate package` and the throughput bench (the verifier's package gate, Q0061).

### As built: Amendment 1 (the 1-second cap, Q0112 2(c))

Built on branch `snapshot-cap` from `5f9f63e9` (main `802df056`, checksum schema 47), 2026-11-25, in `c6b2de8c`.

- **Where the cap is authored.** `assets/catalog/units.ron`, a top-level field beside `units` (the file whose
  weapons author `attack_visibility`): `attack_snapshot_cap_ticks: Some(25)`, with its comment block at the head of
  the file. `UnitCatalog::from_ron_str` reads it into `UnitCatalog::attack_snapshot_cap` (`Option<NonZeroU32>`); a
  cap of 0 is refused with `SimCoreError::CatalogInvalidValue { definition: "units.ron", field:
  "attack_snapshot_cap_ticks" }`, which the runtime loader reports against `units.ron`.
- **The rule.** `SimWorld::prune_attack_snapshots` (`world/vision.rs`) also drops a record once
  `current_tick - struck >= cap`, so a snapshot struck in tick t is listed after ticks t to t + 24 and gone after tick
  t + 25. The record already carried `struck`; nothing was added to it.
- **Tests.** Red first (`~/scratch/snapshot-cap/red/attack_snapshot_tests.log`: the four cap tests failed, the record
  outliving 25, 40 and 200 ticks and a cap of 0 loading). In `attack_snapshot_tests.rs`:
  `a_snapshot_left_alone_ends_when_the_cap_is_reached` (shipped 25: listed while younger, gone on exactly struck + 25),
  `a_later_strike_before_the_cap_restarts_it` (moved pose, listed past the first strike's cap, gone the cap after the
  last), `sight_still_ends_a_snapshot_before_the_cap`, `the_cap_is_read_from_the_catalog` (scratch copies of the
  catalog directory with `Some(40)` and `None`), `a_cap_of_zero_is_refused`; P-5 to P-8 and P-10 now run on a scratch
  catalog with a cap of 1,000,000 ticks, so only sight ends their image. Unit tests: `catalog.rs`
  `the_attack_snapshot_cap_is_optional_and_never_zero`, `world/vision.rs`
  `an_unseen_snapshot_is_dropped_when_its_cap_is_reached`.
- **Proof.** Both goldens (`golden_replay_tests`, 4 tests) and `DEFAULT_SANDBOX_CHECKSUM` (`fixture_tests`, 3 tests)
  pass unchanged on native debug and release and `wasm32-wasip1` release and debug on `c6b2de8c`; no fixture,
  checksum constant or schema line changed (logs `~/scratch/snapshot-cap/proof/`).

Deviations from the amendment:

1. **An absent field means no cap.** The amendment asks for a validated authored field; leaving it optional
   (absent or `None` = "until seen", 5.0.14's behaviour) keeps the 29 test-built `units.ron` fragments valid. The
   shipped file authors 25, and a unit test requires it.
2. **`UnitCatalog::set_attack_snapshot_cap`** (public, like `set_movement_rules`) was added for tests that hold an
   image longer than the cap: the client's `a_held_attack_snapshot_changes_nothing_on_running_frames` and the
   allocation count's running children make the struck unit invulnerable, so no strike refreshes their image, and they
   lift the cap before staging (they measure a held image, not its lifetime). `a_refreshed_attack_snapshot_spawns_nothing`
   runs under the shipped cap.
3. **`player_view_tests`' hidden-attacker bench** now orders its victim to hold (it walked out of reach after the
   first strikes, so the shipped cap ended the image within the 200-tick check) and picks an attacker whose period plus
   greatest random delay is below the cap.
4. **The cap is a difference from StarCraft II 5.0.14** (no duration in the data), chosen by the owner in Q0112 2(c);
   it is recorded in the B-183 row. It is not added to `0052-sanctioned-sc2-deviations.md`, which this amendment did
   not name.
5. **Not run here:** `cargo gate package` and the throughput bench (the verifier's package gate).
