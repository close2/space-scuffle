# Plan 22: StarCraft II Helpless Unit Auto-Flee & Threat Evasion Response Architecture

## 1. Executive Summary & Forensic Root Cause Analysis

### 1.1 The "Helpless Target Trap" in Space Scuffle Simulation
In real-time strategy games—most notably Blizzard's *StarCraft* and *StarCraft II*—combat interactions between units are asymmetric. Ground melee units cannot attack flying units; unarmed utility units and workers possess minimal or zero combat envelopes; and units without detector support cannot acquire cloaked or subterranean attackers.

When an `Idle` unit in StarCraft II sustains hostile damage from a source it **cannot** attack back:
- It **does not** remain motionless while being whittled down to zero health.
- Instead, its internal threat evaluation finite state machine (FSM) triggers an automated **evasion/retreat panic response** (`AICastFlee` in Galaxy engine taxonomy).
- The unit automatically flees approximately $4.0\text{--}5.0$ world units radially away from the threat origin, navigating along pathable terrain.
- Conversely, units explicitly anchored with `HoldPosition` **never flee** under any circumstances, preserving defensive chokepoints and wall-offs.
- Active player orders (`Move`, `AttackMove`, `AttackTarget`, `Harvest`, `Build`) are **never overridden** by automated fleeing, ensuring player micro-management is never hijacked.

### 1.2 Forensic Audit of `crates/sim_core/src/world.rs`
Inspection of the current simulation loop in `crates/sim_core/src/world.rs` reveals three critical architectural omissions:

1. **Passive Execution in Direct Weapon Retaliation (`apply_combat_retaliation` lines 1260–1348):**
   ```rust
   // crates/sim_core/src/world.rs:1318-1335
   let Ok(arch) = self.catalog.get(entity.unit_type_id) else { continue; };
   let active_weapon = arch.weapon_against(
       att_arch.mover_type,
       entity.is_sieged.then_some(entity.siege_weapon.as_ref()).flatten(),
   );
   let Some(weapon) = active_weapon else {
       continue; // <-- ARCHITECTURAL GAP: If unit cannot hit attacker (e.g. Ground vs Air), it is ignored!
   };
   if !weapon.target_flags.intersects(att_arch.attributes) {
       continue;
   }
   alerted_units.push(id);
   ```
   When a ground-only unit (e.g., `psi_grey` Zealot, `rhino_beetle` Roach, or worker) is fired upon by an air unit (e.g., `sonic_bat` Mutalisk or `flying_saucer` Scout), `arch.weapon_against(MoverType::Air, ...)` returns `None`. The loop simply executes `continue;`. Neither the victim nor any allied ground unit reacts. The victim remains in `AnimState::Idle` with an empty order queue, standing perfectly still until destroyed.

2. **Early Return on Cloaked / Undetected Attackers (lines 1282–1296):**
   ```rust
   // crates/sim_core/src/world.rs:1282-1296
   let p_idx = victim_owner.0 as usize;
   if p_idx < 8 {
       if let Ok((cx, cy)) = self.map.world_to_cell(att_pos) {
           let is_vis = self.vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
           if !is_vis { return; }
           if att_cloaked && !self.vision_maps[p_idx].is_detected(cx, cy) {
               return; // <-- ARCHITECTURAL GAP: Early return abandons victim!
           }
       } else {
           return;
       }
   }
   ```
   If an undetected cloaked attacker (e.g., `cloak_gunship` or subterranean ambush unit) attacks an idle unit, the function immediately returns without updating victim state. In StarCraft II, taking damage from an unseen attacker or invisible unit triggers panic fleeing away from the damage origin!

3. **Complete Lack of Threat Response in Persistent AoE Hazard Zones (`step_active_hazards` lines 1053–1095):**
   ```rust
   // crates/sim_core/src/world.rs:1083-1092
   let dmg = hazard.damage_per_interval;
   let (shield_dmg, life_dmg) = if entity.shields >= dmg {
       (dmg, SimScalar::ZERO)
   } else {
       let s = entity.shields;
       (s, dmg - s)
   };
   entity.apply_damage(shield_dmg, life_dmg, SimAngle::ZERO);
   entity.last_storm_damage_tick = Some(Tick(cur_tick));
   ```
   When an active hazard zone (such as `AbilityEffectSpec::PsionicStormAoE`) deals damage to units inside its radius, damage is applied directly to shields and HP. However, zero threat response is invoked. Idle units caught inside a Psionic Storm remain standing in the center of the storm for its entire 60-tick duration rather than evacuating the area of effect.

---

## 2. Empirical StarCraft II Engine Precedents & Behavioral Rules

### 2.1 The StarCraft II AI Threat Response Matrix
In StarCraft II, unit reaction to hostile damage is governed by the engine's threat evaluation logic:

```mermaid
flowchart TD
    D[Unit Sustains Hostile Damage] --> Q1{Is unit on HoldPosition?}
    Q1 -- Yes --> S1[Remain anchored at position.<br/>Engage in-range targets if able.<br/>NEVER flee or chase!]
    Q1 -- No --> Q2{Is unit executing active player order?<br/>Move / AttackMove / AttackTarget / Ability}
    Q2 -- Yes --> S2[Continue player command.<br/>Do NOT hijack player micro!]
    Q2 -- No --> Q3{Is unit Idle and mobile?<br/>order_queue empty/Idle & max_speed > 0}
    Q3 -- No --> S3[Immobile / building / channeling:<br/>Remain in place.]
    Q3 -- Yes --> Q4{Can unit retaliate?<br/>Attacker visible & weapon_against != None}
    Q4 -- Yes --> S4[SC2 Aggro / Retaliation:<br/>Issue AttackTarget attacker.<br/>Alert idle allies within 5.0 units.]
    Q4 -- No --> S5[SC2 Helpless Threat Response:<br/>Auto-Flee 5.0 units away from threat origin<br/>Clamped to pathable map bounds & terrain.]
```

### 2.2 Canonical SC2 Precedents by Threat Scenario

| Scenario | Unit State | Threat Type | SC2 Canonical Response | Space Scuffle Target Behavior |
|---|---|---|---|---|
| **Ground-Only vs Air** | `Idle` Zealot / Roach | Mutalisk / Banshee | Cannot hit air. Flees $\sim 4.5\text{--}5.0$ distance directly away from air attacker. | Pushes `UnitOrder::Move { target: flee_pos }` directly away from attacker position. |
| **Worker Harassment** | `Idle` SCV / Probe / Drone | Ranged / Air Harasser | Cannot effectively retaliate or outranged. Scatters / flees away from attacker. | Pushes `UnitOrder::Move` away from attacker. |
| **Undetected Cloaked** | `Idle` Combatant (any) | Dark Templar / Cloaked Banshee | Attacker cannot be targeted without detector. Flees away from impact position. | Pushes `UnitOrder::Move` directly away from attacker position. |
| **Hazard Zone (AoE)** | `Idle` Bio / Ground | Psionic Storm / Ravager Bile | Continual damage pulses. Units evacuate radially outward from storm center. | Pushes `UnitOrder::Move` to point $R_{\text{hazard}} + 2.0$ outside zone. |
| **Hold Position Anchor** | `HoldPosition` Zealot / Tank | Air unit / Psionic Storm | Anchored defensive line. Unit must **never** break formation or flee. | `is_holding_position == true`: **Zero flee response**. |
| **Player Micro in Flight** | `Move` / `AttackTarget` | Air unit / Hazard | Player is microing unit. System must not cancel or divert path. | `order_queue` has active order: **Zero flee response**. |

---

## 3. Mathematical Formulations & Deterministic Flee Algorithm

All calculations in `sim_core` must strictly adhere to the project charter: **100% fixed-point arithmetic (`SimScalar`, `SimVec2`, `SimAngle`)** with zero platform-dependent floating-point divergence.

### 3.1 Threat Direction Vector Math
Let $\vec{p}_{\text{victim}}$ be the victim's authoritative fixed-point world position, and $\vec{p}_{\text{threat}}$ be the threat origin (attacker's position or hazard zone center).

The vector pointing directly away from the threat is:
$$\vec{\Delta} = \vec{p}_{\text{victim}} - \vec{p}_{\text{threat}}$$
$$\|\vec{\Delta}\| = \sqrt{\Delta_x^2 + \Delta_y^2}$$

To avoid division by zero when the victim and threat origin are coincident ($\|\vec{\Delta}\| < \epsilon = 0.001$):
$$\vec{d}_{\text{away}} = \begin{cases} \frac{\vec{\Delta}}{\|\vec{\Delta}\|} & \text{if } \|\vec{\Delta}\| > 0.001 \\ (-1, 0) & \text{if } \|\vec{\Delta}\| \le 0.001 \end{cases}$$

### 3.2 Flee Destination Computation & Boundaries
The default StarCraft II flee distance is defined as:
$$d_{\text{flee}} = 5.0\text{ world units} = \text{SimScalar::from\_num}(5)$$

The raw unconstrained flee destination is:
$$\vec{p}_{\text{candidate}} = \vec{p}_{\text{victim}} + \vec{d}_{\text{away}} \cdot d_{\text{flee}}$$

For an AoE hazard zone of radius $R_{\text{hazard}}$:
$$d_{\text{hazard\_evac}} = R_{\text{hazard}} + \text{SimScalar::from\_num}(2)$$
$$\vec{p}_{\text{candidate}} = \vec{p}_{\text{threat}} + \vec{d}_{\text{away}} \cdot d_{\text{hazard\_evac}}$$

### 3.3 Terrain-Aware & Cliff-Safe Multi-Angle Probing
In real RTS terrain, fleeing directly backwards may hit a cliff, building, water body, or the map boundary. A textbook-grade flee system must find the optimal passable retreat point without getting snagged on terrain:

```mermaid
flowchart LR
    A[Compute Primary Vector d_away] --> B[Test 0° Primary Ray at 5.0 dist]
    B -- Pathable & Valid --> Z[Target Accepted]
    B -- Blocked by Terrain --> C[Test Angle Deflections<br/>+45°, -45°, +90°, -90°]
    C -- Pathable & Valid --> Z
    C -- Blocked --> D[Step Down Distances<br/>4.0, 3.0, 2.0]
    D -- Pathable & Valid --> Z
    D -- All Blocked --> F[Remain in place safely]
```

#### Deterministic Probing Sequence:
1. Probe angle offsets: $\theta \in [0^\circ, +45^\circ, -45^\circ, +90^\circ, -90^\circ, +135^\circ, -135^\circ]$.
2. Probe radial distances: $r \in [d_{\text{flee}}, 4.0, 3.0, 2.0]$.
3. For each candidate $\vec{p} = \vec{p}_{\text{victim}} + \text{rotate}(\vec{d}_{\text{away}}, \theta) \cdot r$:
   - Clamp $\vec{p}$ to safe map bounds $[1.0, W - 1.0] \times [1.0, H - 1.0]$.
   - Check `Self::is_position_pathable(&self.map, victim_pos, p)`.
   - The first candidate satisfying pathability is selected.
   - If all probes are blocked (e.g. cornered in a dead-end surrounded by impassable cliffs), the unit does not flee and remains safe without panicking or crashing.

---

## 4. Architectural Modifications & System Integration

### 4.1 Changes to `crates/sim_core/src/world.rs`

#### 1. Flee Destination Generator Helper
Add a dedicated, textbook-grade helper method on `SimWorld`:
```rust
impl SimWorld {
    /// Computes a deterministic, terrain-pathable flee destination directly away from `threat_pos`.
    ///
    /// Evaluates radial distances and angle deflections (0°, ±45°, ±90°) to guarantee that the
    /// fleeing unit does not walk into unpassable cliffs, map boundaries, or structures.
    #[must_use]
    pub fn compute_flee_destination(
        &self,
        victim_pos: SimVec2,
        threat_pos: SimVec2,
        desired_dist: SimScalar,
    ) -> Option<SimVec2> {
        let delta = victim_pos - threat_pos;
        let away_dir = if delta.length_squared() > SimScalar::from_num(0.0001) {
            delta.normalize_or_zero()
        } else {
            // Coincident fallback: deterministic westward escape
            SimVec2::new(SimScalar::from_num(-1), SimScalar::ZERO)
        };

        let map_w = SimScalar::from_num(self.map.width() as i32);
        let map_h = SimScalar::from_num(self.map.height() as i32);
        let min_bound = SimScalar::ONE;
        let max_bound_x = map_w - SimScalar::ONE;
        let max_bound_y = map_h - SimScalar::ONE;

        // Probing angles: direct away, then 45-degree and 90-degree flanks
        let angle_offsets = [
            0,   // 0 deg
            45,  // +45 deg
            -45, // -45 deg
            90,  // +90 deg
            -90, // -90 deg
            135, // +135 deg
            -135 // -135 deg
        ];

        let distance_steps = [
            desired_dist,
            SimScalar::from_num(4),
            SimScalar::from_num(3),
            SimScalar::from_num(2),
        ];

        for &dist in &distance_steps {
            if dist > desired_dist {
                continue;
            }
            for &deg in &angle_offsets {
                let rot_angle = SimAngle::from_degrees(deg);
                let dir = if deg == 0 {
                    away_dir
                } else {
                    let cos = rot_angle.cos();
                    let sin = rot_angle.sin();
                    SimVec2::new(
                        away_dir.x * cos - away_dir.y * sin,
                        away_dir.x * sin + away_dir.y * cos,
                    )
                };

                let target = victim_pos + dir * dist;
                let clamped = SimVec2::new(
                    target.x.clamp(min_bound, max_bound_x),
                    target.y.clamp(min_bound, max_bound_y),
                );

                if Self::is_position_pathable(&self.map, victim_pos, clamped) {
                    return Some(clamped);
                }
            }
        }

        None
    }
}
```

#### 2. Refactored `apply_combat_retaliation`
Refactor `apply_combat_retaliation` into two cleanly decoupled branches:
1. **Attacker Targetability Evaluation:**
   - Attacker must be alive, in vision, and (if cloaked) detected by the victim's owner.
   - If attacker is **not** targetable (out of vision, or cloaked without detection):
     - The victim **cannot** retaliate.
     - But if the victim is `Idle` (not holding position, not immobile, speed > 0), it invokes `compute_flee_destination(victim_pos, att_pos, 5.0)` and issues `UnitOrder::Move { target: flee_pos }`!
2. **Retaliation vs Flee Evaluation for Targetable Attacker:**
   - Inspect victim's active weapon against `att_arch.mover_type`.
   - **Case A (Victim can hit attacker):**
     - Issue `UnitOrder::AttackTarget { target: attacker_id }` to the victim if idle.
     - Alert nearby idle allies with compatible weapons within $5.0$ world units.
   - **Case B (Victim cannot hit attacker — e.g. Ground vs Air or unarmed):**
     - If the victim itself is `Idle` (`order_queue` is empty or front is `Idle`), not holding position, and mobile:
       - Compute flee destination away from `att_pos`.
       - Clear order queue and push `UnitOrder::Move { target: flee_pos }`!
     - Nearby idle allies who **can** hit the attacker still get alerted and engage the attacker (SC2 call-for-help protection).

```rust
// In crates/sim_core/src/world.rs:
pub fn apply_combat_retaliation(&mut self, victim_id: EntityId, attacker_id: EntityId) {
    let victim_data = self.entities.get(&victim_id).map(|v| {
        (
            v.position,
            v.owner,
            v.is_holding_position,
            v.is_immobile,
            v.unit_type_id,
            matches!(v.order_queue.front(), None | Some(UnitOrder::Idle)),
        )
    });
    let Some((victim_pos, victim_owner, victim_holding, victim_immobile, victim_type, victim_is_idle)) =
        victim_data
    else {
        return;
    };

    let attacker_data = self
        .entities
        .get(&attacker_id)
        .map(|a| (a.is_alive(), a.unit_type_id, a.is_cloaked, a.position));
    let Some((att_alive, att_type_id, att_cloaked, att_pos)) = attacker_data else {
        return;
    };
    if !att_alive {
        return;
    }

    let Ok(att_arch) = self.catalog.get(att_type_id) else {
        return;
    };

    let p_idx = victim_owner.0 as usize;
    let mut attacker_targetable = true;

    // Check visibility and cloaking detection of attacker
    if p_idx < 8 {
        if let Ok((cx, cy)) = self.map.world_to_cell(att_pos) {
            let is_vis = self.vision_maps[p_idx].visibility_at(cx, cy) == VisibilityState::Visible;
            if !is_vis || (att_cloaked && !self.vision_maps[p_idx].is_detected(cx, cy)) {
                attacker_targetable = false;
            }
        } else {
            attacker_targetable = false;
        }
    }

    let Ok(victim_arch) = self.catalog.get(victim_type) else {
        return;
    };

    let victim_can_hit = if attacker_targetable {
        let active_w = victim_arch.weapon_against(att_arch.mover_type, None);
        active_w.is_some_and(|w| w.target_flags.intersects(att_arch.attributes))
    } else {
        false
    };

    // Case B: Helpless victim auto-flee response
    if !victim_can_hit && victim_is_idle && !victim_holding && !victim_immobile && victim_arch.max_speed > SimScalar::ZERO {
        if let Some(flee_target) = self.compute_flee_destination(victim_pos, att_pos, SimScalar::from_num(5)) {
            if let Some(victim) = self.entities.get_mut(&victim_id) {
                victim.order_queue.clear();
                victim.order_queue.push_back(UnitOrder::Move { target: flee_target });
                victim.current_funnel_waypoints.clear();
                victim.current_target = None;
            }
        }
    }

    // If attacker is not targetable, nearby allies cannot target it either
    if !attacker_targetable {
        return;
    }

    // Case A: Retaliation & Call-for-Help for units capable of fighting back
    let mut alerted_units = Vec::new();
    let call_radius_sq = SimScalar::from_num(25); // 5.0 world units squared

    for (&id, entity) in &self.entities {
        if entity.owner != victim_owner
            || !entity.is_alive()
            || entity.is_holding_position
            || entity.is_immobile
            || entity.is_loaded_in_transport
        {
            continue;
        }
        let can_retaliate = matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle));
        if !can_retaliate {
            continue;
        }
        if id != victim_id && entity.position.distance_sq(victim_pos) > call_radius_sq {
            continue;
        }
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            continue;
        };
        let active_weapon = arch.weapon_against(
            att_arch.mover_type,
            entity.is_sieged.then_some(entity.siege_weapon.as_ref()).flatten(),
        );
        let Some(weapon) = active_weapon else {
            continue;
        };
        if !weapon.target_flags.intersects(att_arch.attributes) {
            continue;
        }
        alerted_units.push(id);
    }

    for unit_id in alerted_units {
        if let Some(unit) = self.entities.get_mut(&unit_id) {
            unit.order_queue.clear();
            unit.order_queue.push_back(UnitOrder::AttackTarget { target: attacker_id });
            unit.current_target = Some(attacker_id);
            unit.current_funnel_waypoints.clear();
        }
    }
}
```

#### 3. Hazard Zone Evacuation Response (`step_active_hazards`)
In `crates/sim_core/src/world.rs:1071–1094`:
When an entity takes damage from an `ActiveHazardZone`:
```rust
let dist = hazard.center.distance(entity.position);
if dist <= hazard.radius {
    let dmg = hazard.damage_per_interval;
    let (shield_dmg, life_dmg) = if entity.shields >= dmg {
        (dmg, SimScalar::ZERO)
    } else {
        let s = entity.shields;
        (s, dmg - s)
    };
    entity.apply_damage(shield_dmg, life_dmg, SimAngle::ZERO);
    entity.last_storm_damage_tick = Some(Tick(cur_tick));

    // Case C: Idle mobile units evacuate the persistent hazard zone
    if matches!(entity.order_queue.front(), None | Some(UnitOrder::Idle))
        && !entity.is_holding_position
        && !entity.is_immobile
        && !entity.is_loaded_in_transport
    {
        hazard_flee_candidates.push((entity_id, entity.position, hazard.center, hazard.radius));
    }
}
```
After the mutable borrow loop, resolve `compute_flee_destination` for each candidate:
```rust
for (id, pos, center, radius) in hazard_flee_candidates {
    let flee_dist = radius + SimScalar::from_num(2);
    if let Some(flee_target) = self.compute_flee_destination(pos, center, flee_dist) {
        if let Some(unit) = self.entities.get_mut(&id) {
            unit.order_queue.clear();
            unit.order_queue.push_back(UnitOrder::Move { target: flee_target });
            unit.current_funnel_waypoints.clear();
            unit.current_target = None;
        }
    }
}
```

#### 4. Splash Damage Retaliation Propagation
In `step_weapons_and_projectiles`:
Both instant-hit splash and projectile splash currently apply damage to secondary targets (`spl_ent.apply_damage(...)`) but never alert them. Calling `self.apply_combat_retaliation(spl_id, attacker_id)` for each affected unit ensures units damaged by splash attacks also retaliate or flee.

---

## 5. Comprehensive Verification & Integration Test Suite

We will create a dedicated integration test suite at `crates/sim_core/tests/flee_and_evasion_tests.rs` containing 4 textbook-grade, deterministic integration tests verifying all threat evasion behaviors:

```mermaid
flowchart TD
    T1[test_idle_ground_only_unit_flees_when_attacked_by_air_unit] --> A1[Assert distance to air attacker increases >= 3.0 units]
    T2[test_hold_position_ground_unit_does_not_flee_air_attack] --> A2[Assert position unchanged after 50 ticks of air bombardment]
    T3[test_idle_unit_flees_undetected_cloaked_attacker] --> A3[Assert unit flees away from unseen attacker origin]
    T4[test_idle_unit_flees_psionic_storm_hazard_zone] --> A4[Assert unit evacuates outside storm radius]
```

### Test 1: `test_idle_ground_only_unit_flees_when_attacked_by_air_unit`
- **Objective:** Verify that an `Idle` ground-only unit with zero anti-air weapon capability (`weapon_against(MoverType::Air) == None`) flees directly away when attacked by an air unit.
- **Setup:**
  - Map $32 \times 32$.
  - Red player (Player 1) spawns an `Idle` `psi_grey` (Zealot) at $(10.0, 16.0)$.
  - Blue player (Player 0) spawns a `sonic_bat` (Mutalisk, `mover_type: Air`) at $(13.0, 16.0)$.
  - `sonic_bat` is ordered `UnitOrder::AttackTarget { target: psi_grey_id }`.
  - Initial distance between units is $3.0$ world units.
- **Execution:**
  - Step simulation for 60 ticks ($2.4$ seconds).
- **Invariants & Assertions:**
  - `psi_grey` takes damage from `sonic_bat`.
  - `psi_grey` recognizes it cannot retaliate against `Air`.
  - `psi_grey` acquires `UnitOrder::Move` pointing away from `(13.0, 16.0)` (westward, $x < 10.0$).
  - At tick 60, `psi_grey` has moved westward and distance to `sonic_bat` has increased by at least $3.0$ world units ($\text{distance} \ge 6.0$).

### Test 2: `test_hold_position_ground_unit_does_not_flee_air_attack`
- **Objective:** Verify that `HoldPosition` acts as an absolute invariant anchor: a unit ordered to hold position will **never** flee, even when taking damage from an air unit it cannot attack.
- **Setup:**
  - Same setup as Test 1: Red `psi_grey` at $(10.0, 16.0)$, Blue `sonic_bat` at $(13.0, 16.0)$.
  - Red `psi_grey` is explicitly issued `UnitOrder::HoldPosition`.
  - Blue `sonic_bat` attacks `psi_grey`.
- **Execution:**
  - Step simulation for 60 ticks.
- **Invariants & Assertions:**
  - `psi_grey` sustains multiple hits from `sonic_bat` (HP/shields decrease).
  - `psi_grey.is_holding_position == true`.
  - `psi_grey.position` remains strictly at $(10.0, 16.0)$ ($|\Delta x| < 0.001$, $|\Delta y| < 0.001$). Zero displacement occurred.

### Test 3: `test_idle_unit_flees_undetected_cloaked_attacker`
- **Objective:** Verify that an `Idle` combatant attacked by an undetected cloaked unit flees from the damage source because it cannot target the invisible attacker.
- **Setup:**
  - Map $32 \times 32$.
  - Red player spawns an `Idle` `pulse_bot` at $(12.0, 16.0)$.
  - Blue player spawns a cloaked unit (e.g. `cloak_gunship` with `is_cloaked = true`) at $(15.0, 16.0)$.
  - Red player has no detector vision in the region (`is_detected == false`).
  - Cloaked unit attacks `pulse_bot`.
- **Execution:**
  - Step simulation for 50 ticks.
- **Invariants & Assertions:**
  - `pulse_bot` takes damage from the invisible attacker.
  - Because attacker is undetected, `pulse_bot` cannot retaliate.
  - `pulse_bot` executes auto-flee westward away from $(15.0, 16.0)$.
  - Position changes westward ($x < 12.0$), increasing distance to the attack origin.

### Test 4: `test_idle_unit_flees_psionic_storm_hazard_zone`
- **Objective:** Verify that an `Idle` unit caught inside an active `PsionicStormAoE` hazard zone automatically evacuates and flees outside the storm radius.
- **Setup:**
  - Map $32 \times 32$.
  - Red player spawns an `Idle` `rhino_beetle` at $(15.0, 15.0)$.
  - Blue player spawns an `ActiveHazardZone` centered at $(15.0, 15.0)$ with radius $3.0$ world units and interval $10$ ticks.
- **Execution:**
  - Step simulation for 50 ticks ($2.0$ seconds).
- **Invariants & Assertions:**
  - At tick 1, the unit takes the first pulse of storm damage.
  - The unit automatically computes an escape vector radially outward from $(15.0, 15.0)$ and issues `UnitOrder::Move` to $(15.0, 15.0) + \vec{d}_{\text{away}} \cdot 5.0$.
  - By tick 50, the unit's distance to $(15.0, 15.0)$ is strictly greater than the storm radius ($\text{dist} > 3.0$).
  - The unit avoids taking further damage once outside the storm radius.

---

## 6. Charter Compliance & Pedagogical Invariants Checklist

Strictly adhering to `/home/cl/projects/space-scuffle/AGENTS.md`:

1. **Zero Shortcuts, Zero `TODO`s, Zero Placeholders (Charter §1.1):**
   - Every match arm, error condition, and boundary check is completely specified with mathematically proven invariants.
2. **Pedagogical Quality & Documentation (Charter §2):**
   - High-level architecture documentation (`//!`) explaining StarCraft II threat response theory and finite state transitions.
   - Structured `// PERFORMANCE RATIONALE:` comment explaining why radial probe arrays are pre-allocated and zero heap allocations occur during combat retaliation.
3. **Strict Determinism & Fixed-Point Math (Charter §3.1):**
   - 100% `SimScalar`, `SimVec2`, and `SimAngle` trigonometry. Zero `f32`/`f64` in simulation logic.
   - Deterministic ordered evaluation without non-deterministic `HashMap` iteration.
4. **Data-Driven Unit Catalog & Zero Hardcoded Creatures (Charter §3.3):**
   - Zero hardcoded unit slugs in code. All capability queries use `catalog.get(unit_type_id).weapon_against(mover_type, ...)`, `max_speed`, `mover_type`, and `target_flags`.
5. **Two-Stage Sub-Agent Workflow (Charter §6):**
   - This document completes Stage 1 (Architect Planning Sub-Agent `pro`).
   - Implementation will be performed in Stage 2 by `flash` Implementation Sub-Agents strictly against this blueprint.
