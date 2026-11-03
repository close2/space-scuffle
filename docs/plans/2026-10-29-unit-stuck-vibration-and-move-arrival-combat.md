# Stage 1 Architectural Blueprint: Unit Stuck/Vibration Resolution & Move-Arrival Combat Semantics

**Date:** 2026-10-29  
**Target Crates:** `crates/sim_core` (`src/movement/locomotion.rs`, `src/steering.rs`, `src/world.rs`, and integration tests)  
**Status:** Approved for Stage 2 Implementation

---

## 1. Executive Summary & User Questions Answered

This blueprint resolves the root causes of the `LiveBattle` unit-stuck/vibration defects reported by the user and clarifies/completes the StarCraft II movement-vs-combat order semantics:

1. **StarCraft II Move (`Right-Click`) vs. Attack-Move (`A + Left-Click`) Semantics:**
   - **While en route (`UnitOrder::Move { target }`):** In StarCraft II (and `sim_core`), a standard right-click `Move` order commands units to prioritize locomotion to `target` without stopping along the way to chase or retaliate against attackers (except for units whose turret/weapon explicitly allows firing while moving, if any). By contrast, `UnitOrder::AttackMove { target }` scans for enemies via `acquire_targets` on every tick while advancing.
   - **Upon reaching the destination (`Move` completes $\to$ `UnitOrder::Idle`):** In StarCraft II, as soon as a squad arrives at its `Move` destination, every arrived unit transitions from `Move` to `Idle`. Once `Idle`, units **immediately** auto-acquire enemies in weapon range (`acquire_targets`, `world.rs` line 1545) and retaliate when attacked (`apply_combat_retaliation_prevalidated`, `world.rs` line 2534).
   - **Why units in `Space Scuffle` failed to attack even after arriving at a right-clicked location:** Because of a route-Drain bug in `prepare_actor_motion` (`crates/sim_core/src/movement/locomotion.rs` lines 328–382), only the **single first unit** in a squad whose center physically reached within `speed` (`0.12 m`) of `target` popped the final funnel waypoint `target` and transitioned to `UnitOrder::Idle`. Every other unit in the squad ($2\dots N$) remained stuck with `current_funnel_waypoints == [target]` (`proposal.route(actor).len() == 1`), which bypassed `check_cluster_arrival_with` (gated behind `if proposal.route(actor).is_empty()`). Consequently, units $2\dots N$ **never completed their `Move` order**, never transitioned to `Idle`, vibrated endlessly against the lead unit, and **never fired back or auto-acquired enemies** at the destination.

2. **Four Interlocking Root Causes of Unit Stuck & Vibration Behavior:**
   - **Root Cause A — Final-Leg Funnel Waypoint Blocks Cluster Arrival (`locomotion.rs` lines 328–382):** Ground pathfinding populates `current_funnel_waypoints = [..., target]`, ending with `target` itself. On the final path segment toward `target`, `proposal.route(actor)` has length `1` (`[target]`), not `0`. Because `check_cluster_arrival_with` is only called when `proposal.route(actor).is_empty()`, trailing squad members blocked by already-settled teammates never invoke `check_cluster_arrival_with`, never pop `UnitOrder::Move`, and oscillate forever at the destination.
   - **Root Cause B — Intermediate Funnel Waypoint Pinhole Trap (`locomotion.rs` lines 378–382):** Intermediate funnel waypoints (such as ramp/cliff corners) require `distance(actor.position, next) <= speed` (`~0.08–0.12 m`) to pop. When a medium/large unit (`radius = 0.75 m`) steers around a teammate near a ramp corner via `compute_unified_local_steering`, its center passes `0.4–0.9 m` away from the corner waypoint and already has clear line-of-sight to the next waypoint, yet it turns backward to touch the `0.12 m` pinhole, colliding head-on with trailing teammates and vibrating in place.
   - **Root Cause C — Direct-Line Cliff Wedging During `AttackTarget` Pursuit (`locomotion.rs` lines 489–520):** In `prepare_pursuit`, out-of-range `Ground` units pursuing an `AttackTarget` compute local steering straight toward `target.position` without checking `pathfinder.segment_is_passable`. When a cliff edge or ramp separates attacker and target, the unit walks straight into the impassable cliff edge and vibrates via axis-aligned `prepare_slide`.
   - **Root Cause D — Moving-vs-Stationary Friendly Push Overlap, `0.05` Deadzone Sign-Flipping & Unprotected Engaged Units (`steering.rs` lines 448–501 & `world.rs` lines 2253–2270):**
     1. In `world.rs` line 2268–2270, a stationary friendly Marine (`push_priority = 1`, `radius = 0.45`) actively firing at an enemy was not protected from being pushed by an approaching friendly Rhino Beetle (`push_priority = 2`, `radius = 0.75`) that was not yet in melee range (`priority_b >= priority_a` was `false`), and `is_engaged` ignored stationary units whose `current_target` was auto-acquired from `Idle`.
     2. In `steering.rs` lines 455–462, 472–479, and 494–501, the perpendicular push direction `perp` overrides the geometric sign of `dot` with `if a.id.0 < b.id.0` inside a `0.05` deadzone (`|dot| < 0.05`). Whenever the `EntityId` ordering disagrees with the geometric sign of `dot`, crossing `|dot| = 0.05` flips `perp` by $180^\circ$ and pushes the idle unit **back toward the mover's centerline**, creating a high-frequency mechanical oscillator and allowing larger units to walk directly on top of smaller units.

---

## 2. Detailed Root-Cause Analysis & Mathematical Formulation

### 2.1 Root Cause A: Final-Leg Cluster Arrival & Multi-Ring Squad Settlement

In [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L328-L387), `prepare_actor_motion` currently executes:

```rust
if proposal.route(actor).is_empty() {
    if distance(actor.position, target).map_err(|e| arithmetic(id, e))? <= speed {
        ...
        proposal.finish();
        return Ok(MotionAction::Commit(proposal));
    }
    if check_cluster_arrival_with(
        Some(id),
        actor.position,
        policy.radius,
        target,
        &self.movement.obstacles[policy.layer.index()],
        |witness| {
            policy.permits_segment(&self.map, actor.position, witness.pos)
                && policy.permits_segment(&self.map, witness.pos, target)
        },
    )
    .map_err(|source| MovementInvariant::PushGeometry { entity: id, source })?
    {
        proposal.finish();
        return Ok(MotionAction::Commit(proposal));
    }
    if policy.layer == MovementLayer::Ground
        && !self.pathfinder.segment_is_passable(...)
    {
        let mut route = self.pathfinder.find_path_with_vision(&self.map, &req);
        ...
        proposal.replacement = Some(route);
    }
}
```

When a Ground squad is ordered to move across a ramp or around an obstacle to `target`, `find_path_with_vision` returns `route = [w_0, w_1, ..., target]`, where the last element is `target`. Once the squad clears the ramp, `proposal.route(actor)` is `[target]` (length `1`).
- The first unit to reach `distance(actor.position, target) <= speed` increments `proposal.remove_prefix += 1` (line 380), making its `proposal.route(actor)` empty, and finishes on line 386.
- Every other unit in the squad ($2\dots N$) is blocked by the settled first unit at distance $d \ge r_1 + r_2 \ge 0.90\text{ m} \gg \text{speed}$. Thus `distance(actor.position, next) <= speed` is `false`, `proposal.route(actor)` remains `[target]` (length `1`), and `if proposal.route(actor).is_empty()` is **never entered**.
- Furthermore, in [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L295-L304), `check_cluster_arrival_with` checks:
  $$\lVert \mathbf{p}_{\text{obs}} - \mathbf{p}_{\text{target}} \rVert \le (r_{\text{unit}} + r_{\text{obs}}) + 1.5\text{ m}$$
  For a squad of 8–12 units (including Rhino Beetles with $r = 0.75\text{ m}$ and Marines with $r = 0.45\text{ m}$), 2nd-ring settled witnesses stand at $1.8\text{--}2.5\text{ m}$ from $\mathbf{p}_{\text{target}}$, so 3rd-ring arrivals need the witness-to-target cluster radius to be $(r_{\text{unit}} + r_{\text{obs}}) + 2.5\text{ m}$ (while keeping the neighbor touch margin at `SimScalar::from_f32_lossy(0.15)` so all existing boundary tests in `checked_movement_tests.rs` remain bit-for-bit preserved).

**Architectural Fix:**
1. In `prepare_actor_motion` (`locomotion.rs`), check direct target arrival (`distance(actor.position, target) <= speed`) and `check_cluster_arrival_with` whenever the actor is on the final leg of its route (`proposal.route(actor).len() <= 1`), not only when `proposal.route(actor).is_empty()`.
2. Only invoke initial pathfinding (`if proposal.route(actor).is_empty() && policy.layer == MovementLayer::Ground && !self.pathfinder.segment_is_passable(...)`) when `proposal.route(actor).is_empty()`.
3. In `check_cluster_arrival_with` (`steering.rs` line 299), widen the settled witness-to-target cluster radius from `SimScalar::from_f32_lossy(1.5)` to `SimScalar::from_f32_lossy(2.5)` while keeping the unit-to-witness touch margin at `SimScalar::from_f32_lossy(0.15)`.

---

### 2.2 Root Cause B: Intermediate Funnel Waypoint Corner-Rounding & String-Pulling

In [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L378-L382):
```rust
if let Some(&next) = proposal.route(actor).first() {
    if distance(actor.position, next).map_err(|e| arithmetic(id, e))? <= speed {
        proposal.remove_prefix += 1;
    }
}
```
When `proposal.route(actor).len() > 1`, `next = route[0]` is an **intermediate** funnel waypoint (such as a ramp entrance or corner vertex), and `route[1]` is the subsequent waypoint.
- If `actor` (e.g., a Rhino Beetle with `policy.radius = 0.75 m`) is deflected laterally by `0.5 m` while avoiding another unit at the ramp entrance, requiring `distance(actor.position, next) <= speed` (`0.085 m`) forces the Beetle to turn backward against oncoming squad traffic to touch the exact corner vertex.
- Instead, when `proposal.route(actor).len() > 1`, an intermediate waypoint `route[0]` has served its purpose and must be popped (`proposal.remove_prefix += 1`) as soon as **either**:
  1. The actor's collision body reaches the intermediate waypoint:
     $$\text{distance}(\mathbf{p}_{\text{actor}}, \mathbf{w}_0) \le r_{\text{actor}} + \text{speed}$$
     and the segment from $\mathbf{p}_{\text{actor}}$ to $\mathbf{w}_1$ is permitted by terrain (`policy.permits_segment(&self.map, actor.position, route[1])`), **OR**
  2. For `MovementLayer::Ground`, string-pulling lookahead confirms that the direct path from $\mathbf{p}_{\text{actor}}$ to $\mathbf{w}_1$ is already passable at the unit's clearance (`self.pathfinder.segment_is_passable(&self.map, actor.position, route[1], req.clearance, req.known_structures)`) and `policy.permits_segment(&self.map, actor.position, route[1])` is true, and the unit is within `policy.radius * 2 + speed` of $\mathbf{w}_0$.

This prevents units from ever turning backward to touch an intermediate ramp/corner pinhole after local avoidance has steered them around a neighbor.

---

### 2.3 Root Cause C: Terrain-Aware Pathfinding in `prepare_pursuit`

In [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L489-L520), `prepare_pursuit` currently steers out-of-range `AttackTarget` units straight toward `target.position`:
```rust
let direction = compute_unified_local_steering(
    id,
    actor.position,
    policy.radius,
    target.position,
    Some(target_id),
    &self.movement.obstacles[policy.layer.index()],
)
```
When a Ground unit has an `AttackTarget` order against an enemy on a different cliff level or across an impassable cliff corner, the direct segment `(actor.position, target.position)` is blocked by cliff cells. Steering directly toward `target.position` wedges the unit against the cliff edge where `prepare_slide` oscillates along the cliff wall.

**Architectural Fix in `prepare_pursuit`:**
- When `policy.layer == MovementLayer::Ground`, determine the pursuit steering goal `steering_goal`:
  - Construct the unit's `UnitClearance` from `policy.radius` (matching `prepare_actor_motion`: `<= 0.4` $\to$ `Small`, `<= 0.8` $\to$ `Medium`, else `Large`) and `known_structures` from `vision_aware_pathfinding`.
  - If `!self.pathfinder.segment_is_passable(&self.map, actor.position, target.position, clearance, known_structures)` (or `!policy.permits_segment(&self.map, actor.position, target.position)`), query `self.pathfinder.find_path_with_vision(&self.map, &req)` toward `target.position`.
  - Advance past any leading waypoint `route[0]` within `policy.radius + speed` (when `route.len() > 1`), store the resulting path in `proposal.replacement`, and steer toward the first remaining waypoint `route[0]` (falling back to `target.position` if `route` is empty).
  - When the direct segment IS passable, clear any stale pursuit waypoints (`proposal.clear_route = true`) and steer directly toward `target.position`.

---

### 2.4 Root Cause D: Eliminating the `0.05` Push Deadzone Sign-Flip, Adding Radial+Lateral Separation, and Protecting Engaged Combat Anchors

#### 2.4.1 Why the `0.05` Deadzone in `checked_dynamic_push` Causes High-Frequency Vibration
In [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L448-L501), `checked_dynamic_push` determines the sign of `perp` using:
```rust
let dot = checked_dot(checked_vec_neg(delta)?, perp)?;
if dot.raw().unsigned_abs() < SimScalar::from_f32_lossy(0.05).raw() as u64 {
    if a.id.0 < b.id.0 {
        perp = checked_vec_neg(perp)?;
    }
} else if dot < SimScalar::ZERO {
    perp = checked_vec_neg(perp)?;
}
```
Consider what happens when `a` (moving in $+X$, `perp = (0, +1)`) overlaps `b` (stationary):
- Here $-\mathbf{\Delta} = \mathbf{p}_b - \mathbf{p}_a$, so $\text{dot} = y_b - y_a$.
- Geometric correctness requires `perp` to point into the half-plane containing $\mathbf{p}_b - \mathbf{p}_a$ ($\text{dot} \ge 0$), so that pushing $b$ along `perp` increases $|y_b - y_a|$ and moves $b$ **away** from $a$'s centerline.
- However, inside $|y_b - y_a| < 0.05\text{ m}$:
  - If `a.id.0 < b.id.0` and $y_b - y_a = +0.04\text{ m}$ ($\text{dot} = +0.04 > 0$), the deadzone branch negates `perp` to $(0, -1)$, pushing $b$ **downward toward $a$'s centerline**! If $y_b - y_a$ crosses $+0.05\text{ m}$ on the next tick, the `else if dot < 0` branch does **not** negate `perp` ($(0, +1)$), pushing $b$ upward!
  - Symmetrically, if `a.id.0 > b.id.0` and $y_b - y_a = -0.04\text{ m}$ ($\text{dot} = -0.04 < 0$), the deadzone branch leaves `perp` as $(0, +1)$, pushing $b$ **upward toward $a$'s centerline**, whereas at $y_b - y_a = -0.06\text{ m}$ ($\text{dot} \le -0.05$), `else if dot < 0` negates `perp` to $(0, -1)$!
- Thus, whichever way the `EntityId`s are ordered, one of the two boundaries $\text{dot} = +0.05$ or $\text{dot} = -0.05$ has a **$180^\circ$ sign discontinuity** where the push vector points inward on the inside of $0.05$ and outward on the outside of $0.05$, trapping the unit at $|\text{dot}| \approx 0.05$ in a perpetual 1-tick vibration!

**Fix for `perp` Orientation (`steering.rs` lines 455–462, 472–479, 494–501):**
- In exact Q32.32 fixed-point math, deterministic `EntityId` tie-breaking must **only** occur when `dot == SimScalar::ZERO` (exact collinearity):
  ```rust
  if dot == SimScalar::ZERO {
      if a.id.0 < b.id.0 {
          perp = checked_vec_neg(perp)?;
      }
  } else if dot < SimScalar::ZERO {
      perp = checked_vec_neg(perp)?;
  }
  ```
- Whenever `dot != SimScalar::ZERO`, `perp` is oriented strictly according to `sign(dot)` so that `dot(b.pos - a.pos, perp) > 0` holds unconditionally. There is zero sign discontinuity anywhere off the centerline, and an idle unit is **never** pushed back across the mover's centerline.

#### 2.4.2 Combining Outward Radial Separation with Lateral Yield (`steering.rs` lines 448–481)
When a moving friendly unit $a$ pushes a stationary friendly unit $b$, pure perpendicular displacement `perp` has zero component along $-\text{separation}$ when $b$ is nearly in front of $a$, allowing a fast or large unit $a$ to step onto $b$'s center before $b$ clears the lane.
- By blending outward radial separation (`checked_vec_neg(separation)` for $b$, or `separation` for $a$) with lateral yield (`perp`):
  ```rust
  let outward = checked_vec_neg(separation)?;
  let yield_dir = checked_normalize(checked_vec_add(
      checked_vec_scale(outward, SimScalar::HALF)?,
      checked_vec_scale(perp, SimScalar::from_f32_lossy(0.85))?,
  )?)?;
  return Ok((SimVec2::ZERO, checked_vec_scale(yield_dir, overlap)?));
  ```
  (and symmetrically for `!a_nonzero && b_nonzero && b.push_priority >= a.push_priority` using `separation` + `perp` for $a$ and `SimVec2::ZERO` for $b$):
  1. The moving ally keeps full forward translation (`d_a == SimVec2::ZERO`), preserving all existing unit/integration tests (`test_moving_friendly_pushes_idle_friendly_laterally`, `test_pure_displacement_math`).
  2. The stationary ally yields both laterally (`0.85 * perp`, so `d_b.y != 0`) and radially outward ahead of the mover (`0.5 * outward`), ensuring strictly positive center-to-center separation every tick.

#### 2.4.3 Protecting Stationary Engaged Combat Units from Friendly Shoving (`world.rs` lines 2201–2271)
In [`crates/sim_core/src/world.rs`](../../crates/sim_core/src/world.rs#L2201-L2271):
1. Broaden `is_engaged_a` and `is_engaged_b` so that **any** stationary unit (`velocity == SimVec2::ZERO`) with `current_target.is_some()` (including `Idle` or `Patrol` units that auto-acquired a target and are between shots in `WeaponPhase::Cooldown` or `Ready`) is recognized as engaged in combat:
   ```rust
   let is_engaged_a = matches!(
       entity_a.weapon_phase,
       WeaponPhase::RotatingToTarget { .. }
           | WeaponPhase::Windup { .. }
           | WeaponPhase::Backswing { .. }
   ) || (entity_a.current_target.is_some() && vel_a == SimVec2::ZERO);
   ```
   (and identically for `is_engaged_b` with `entity_b.velocity == SimVec2::ZERO`).
2. When two friendly units (`owner_a == entity_b.owner`) collide and one of them is stationary and actively engaged in combat while the other is moving (not engaged), the stationary firing unit must act as an unpushable combat anchor regardless of whether the moving unit has a higher `push_priority` (e.g., a melee Rhino Beetle with `push_priority = 2` approaching behind a firing Marine with `push_priority = 1`):
   ```rust
   let eff_cannot_be_pushed_a = entity_a.cannot_be_pushed()
       || (owner_a == entity_b.owner
           && is_engaged_a
           && vel_a == SimVec2::ZERO
           && (!is_engaged_b || priority_a >= priority_b));
   let eff_cannot_be_pushed_b = entity_b.cannot_be_pushed()
       || (owner_a == entity_b.owner
           && is_engaged_b
           && entity_b.velocity == SimVec2::ZERO
           && (!is_engaged_a || priority_b >= priority_a));
   ```
   Because `checked_dynamic_push` (`steering.rs` lines 403–439) already handles `b_immobile && !a_immobile` by displacing the moving unit $a$ along `normalize(0.5 * normal + 0.85 * perp) * raw_overlap`, a melee Rhino Beetle approaching behind a firing Marine will now **automatically slide tangentially around the Marine** into an open firing arc instead of walking over the Marine's head!

---

## 3. Step-by-Step Implementation Specification

### Step 1: `crates/sim_core/src/steering.rs`

1. **Multi-Ring Cluster Arrival Limit (`check_cluster_arrival_with`, line 299):**
   - Update the settled witness-to-target distance bound from `SimScalar::from_f32_lossy(1.5)` to `SimScalar::from_f32_lossy(2.5)`:
     ```rust
     arrived = within_distance(
         obs.pos,
         target_pos,
         combined + i128::from(SimScalar::from_f32_lossy(2.5).raw()),
     ) && within_distance(
         unit_pos,
         obs.pos,
         combined + i128::from(SimScalar::from_f32_lossy(0.15).raw()),
     ) && witness_allowed(obs);
     ```
   - Keep `SimScalar::from_f32_lossy(0.15)` unchanged so `legacy_lookahead_and_arrival_raw_thresholds_keep_boundary_branch` in `crates/sim_core/tests/checked_movement_tests.rs` passes without modification.

2. **Eliminate the `0.05` Deadzone Sign Discontinuity & Blend Outward Radial Separation in `checked_dynamic_push` (`steering.rs` lines 448–505):**
   - In the `!b_nonzero && a_nonzero && a.push_priority >= b.push_priority` branch (lines 448–464):
     ```rust
     if !b_nonzero && a_nonzero && a.push_priority >= b.push_priority {
         let move_dir = checked_normalize(a.vel)?;
         let mut perp = if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON {
             SimVec2::new(-move_dir.y, move_dir.x)
         } else {
             SimVec2::Y_AXIS
         };
         let dot = checked_dot(checked_vec_neg(delta)?, perp)?;
         if dot == SimScalar::ZERO {
             if a.id.0 < b.id.0 {
                 perp = checked_vec_neg(perp)?;
             }
         } else if dot < SimScalar::ZERO {
             perp = checked_vec_neg(perp)?;
         }
         let outward = checked_vec_neg(separation)?;
         let yield_dir = checked_normalize(checked_vec_add(
             checked_vec_scale(outward, SimScalar::HALF)?,
             checked_vec_scale(perp, SimScalar::from_f32_lossy(0.85))?,
         )?)?;
         return Ok((SimVec2::ZERO, checked_vec_scale(yield_dir, overlap)?));
     }
     ```
   - In the symmetric `!a_nonzero && b_nonzero && b.push_priority >= a.push_priority` branch (lines 465–481):
     ```rust
     if !a_nonzero && b_nonzero && b.push_priority >= a.push_priority {
         let move_dir = checked_normalize(b.vel)?;
         let mut perp = if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON {
             SimVec2::new(-move_dir.y, move_dir.x)
         } else {
             SimVec2::Y_AXIS
         };
         let dot = checked_dot(delta, perp)?;
         if dot == SimScalar::ZERO {
             if a.id.0 < b.id.0 {
                 perp = checked_vec_neg(perp)?;
             }
         } else if dot < SimScalar::ZERO {
             perp = checked_vec_neg(perp)?;
         }
         let yield_dir = checked_normalize(checked_vec_add(
             checked_vec_scale(separation, SimScalar::HALF)?,
             checked_vec_scale(perp, SimScalar::from_f32_lossy(0.85))?,
         )?)?;
         return Ok((checked_vec_scale(yield_dir, overlap)?, SimVec2::ZERO));
     }
     ```
   - In the both-moving wedge branch (lines 493–501), replace the `0.05` deadzone with exact zero tie-breaking:
     ```rust
     let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
     let dot = checked_dot(delta, perp)?;
     if dot == SimScalar::ZERO {
         if a.id.0 < b.id.0 {
             perp = checked_vec_neg(perp)?;
         }
     } else if dot < SimScalar::ZERO {
         perp = checked_vec_neg(perp)?;
     }
     ```

### Step 2: `crates/sim_core/src/movement/locomotion.rs`

1. **Final-Leg Cluster Arrival & Intermediate Waypoint Corner-Rounding in `prepare_actor_motion` (`locomotion.rs` lines 328–390):**
   - Replace the `if proposal.route(actor).is_empty()` and waypoint-popping block with:
     ```rust
     if proposal.route(actor).len() <= 1 {
         if distance(actor.position, target).map_err(|e| arithmetic(id, e))? <= speed {
             if policy.permits_segment(&self.map, actor.position, target) {
                 proposal.position = target;
                 proposal.finish();
                 return Ok(MotionAction::Commit(proposal));
             }
             proposal.velocity = SimVec2::ZERO;
             return Ok(MotionAction::Commit(proposal));
         }
         if check_cluster_arrival_with(
             Some(id),
             actor.position,
             policy.radius,
             target,
             &self.movement.obstacles[policy.layer.index()],
             |witness| {
                 policy.permits_segment(&self.map, actor.position, witness.pos)
                     && policy.permits_segment(&self.map, witness.pos, target)
             },
         )
         .map_err(|source| MovementInvariant::PushGeometry { entity: id, source })?
         {
             proposal.finish();
             return Ok(MotionAction::Commit(proposal));
         }
     }
     if proposal.route(actor).is_empty()
         && policy.layer == MovementLayer::Ground
         && !self
             .pathfinder
             .segment_is_passable(&self.map, &req, actor.position, target)
     {
         if let Ok(path) = self.pathfinder.find_path_with_vision(&self.map, &req) {
             let mut route = path.waypoints;
             if route.len() > 1
                 && distance(actor.position, route[0]).map_err(|e| arithmetic(id, e))?
                     <= policy
                         .radius
                         .checked_add(speed)
                         .ok_or(MovementMathError::Unrepresentable)
                         .map_err(|e| arithmetic(id, e))?
             {
                 route.remove(0);
             }
             proposal.replacement = Some(route);
         } else {
             proposal.velocity = SimVec2::ZERO;
             return Ok(MotionAction::Commit(proposal));
         }
     }
     if let Some(&first_wp) = proposal.route(actor).first()
         && policy.layer == MovementLayer::Ground
         && (!policy.permits_segment(&self.map, actor.position, first_wp)
             || !self
                 .pathfinder
                 .segment_is_passable(&self.map, &req, actor.position, first_wp))
     {
         if let Ok(path) = self.pathfinder.find_path_with_vision(&self.map, &req) {
             proposal.replacement = Some(path.waypoints);
             proposal.remove_prefix = 0;
         }
     }
     while let Some(&next) = proposal.route(actor).first() {
         let dist_to_next = distance(actor.position, next).map_err(|e| arithmetic(id, e))?;
         let remaining = proposal.route(actor).len();
         if remaining > 1 {
             let corner_reach = policy
                 .radius
                 .checked_add(speed)
                 .ok_or(MovementMathError::Unrepresentable)
                 .map_err(|e| arithmetic(id, e))?;
             let after_next = proposal.route(actor)[1];
             let can_advance = policy.permits_segment(&self.map, actor.position, next)
                 && policy.permits_segment(&self.map, actor.position, after_next)
                 && (policy.layer != MovementLayer::Ground
                     || self.pathfinder.segment_is_passable(
                         &self.map,
                         &req,
                         actor.position,
                         after_next,
                     ))
                 && (dist_to_next <= corner_reach
                     || (policy.layer == MovementLayer::Ground
                         && dist_to_next
                             <= corner_reach
                                 .checked_add(policy.radius)
                                 .ok_or(MovementMathError::Unrepresentable)
                                 .map_err(|e| arithmetic(id, e))?));
             if can_advance {
                 proposal.remove_prefix += 1;
                 continue;
             }
         } else if dist_to_next <= speed {
             proposal.remove_prefix += 1;
         }
         break;
     }
     ```

2. **Terrain-Aware Pursuit Pathfinding in `prepare_pursuit` (`locomotion.rs` lines 489–520):**
   - In `prepare_pursuit`, before calling `compute_unified_local_steering`, compute `steering_goal`:
     ```rust
     let steering_goal = if policy.layer == MovementLayer::Ground {
         let req = self.vision_path_request(actor, policy, target.position);
         if self
             .pathfinder
             .segment_is_passable(&self.map, &req, actor.position, target.position)
             && policy.permits_segment(&self.map, actor.position, target.position)
         {
             proposal.clear_route = true;
             target.position
         } else if let Ok(path) = self.pathfinder.find_path_with_vision(&self.map, &req) {
             let mut route = path.waypoints;
             let corner_reach = policy
                 .radius
                 .checked_add(speed)
                 .ok_or(MovementMathError::Unrepresentable)
                 .map_err(|e| arithmetic(id, e))?;
             while route.len() > 1
                 && distance(actor.position, route[0]).map_err(|e| arithmetic(id, e))?
                     <= corner_reach
                 && policy.permits_segment(&self.map, actor.position, route[1])
                 && self
                     .pathfinder
                     .segment_is_passable(&self.map, &req, actor.position, route[1])
             {
                 route.remove(0);
             }
             let next = route.first().copied().unwrap_or(target.position);
             proposal.replacement = Some(route);
             next
         } else {
             target.position
         }
     } else {
         target.position
     };
     ```
   - Pass `steering_goal` instead of `target.position` as the `goal_pos` argument to `compute_unified_local_steering`.

### Step 3: `crates/sim_core/src/world.rs`

1. **Broaden `is_engaged_a` / `is_engaged_b` and Protect Stationary Engaged Units (`world.rs` lines 2201–2271):**
   - Update `is_engaged_a` (lines 2201–2210):
     ```rust
     let is_engaged_a = matches!(
         entity_a.weapon_phase,
         WeaponPhase::RotatingToTarget { .. }
             | WeaponPhase::Windup { .. }
             | WeaponPhase::Backswing { .. }
     ) || (entity_a.current_target.is_some() && vel_a == SimVec2::ZERO);
     ```
   - Update `is_engaged_b` and `eff_cannot_be_pushed_a` / `eff_cannot_be_pushed_b` (lines 2253–2271):
     ```rust
     let is_engaged_b = matches!(
         entity_b.weapon_phase,
         WeaponPhase::RotatingToTarget { .. }
             | WeaponPhase::Windup { .. }
             | WeaponPhase::Backswing { .. }
     ) || (entity_b.current_target.is_some()
         && entity_b.velocity == SimVec2::ZERO);

     let eff_cannot_be_pushed_a = entity_a.cannot_be_pushed()
         || (owner_a == entity_b.owner
             && is_engaged_a
             && vel_a == SimVec2::ZERO
             && (!is_engaged_b || priority_a >= priority_b));
     let eff_cannot_be_pushed_b = entity_b.cannot_be_pushed()
         || (owner_a == entity_b.owner
             && is_engaged_b
             && entity_b.velocity == SimVec2::ZERO
             && (!is_engaged_a || priority_b >= priority_a));
     ```

---

## 4. Verification & Test Suite Plan

Add comprehensive unit and integration tests verifying every fixed invariant:

1. **Squad Move Across Ramp Completes into `Idle` and Auto-Acquires/Retaliates Against Enemies (`crates/sim_core/tests/collision_and_immobility_tests.rs`):**
   - Spawn a multi-unit mixed squad (e.g., 4 `pulse_bot` Marines and 2 `rhino_beetle` Beetles) on low ground (`CliffLevel(0)`), issue a `UnitOrder::Move { target }` up a ramp onto high ground (`CliffLevel(1)`).
   - Verify that after arriving at the destination cluster:
     - **All** 6 units pop their `Move` order (`order_queue.is_empty()`, `velocity == SimVec2::ZERO`, zero ongoing positional vibration).
     - When an enemy unit is placed within weapon range (or attacks the settled squad), the arrived units immediately auto-acquire and fire back (`current_target.is_some()`).
2. **Higher-Priority Melee Ally Slides Around Stationary Engaged Ranged Ally Without Overlapping (`crates/sim_core/tests/collision_and_immobility_tests.rs`):**
   - Spawn a friendly `pulse_bot` (`push_priority = 1`, `radius = 0.45`, `range = 5.0`) engaged with an enemy target, and a friendly `rhino_beetle` (`push_priority = 2`, `radius = 0.75`, `range = 1.35`) directly behind the `pulse_bot` pursuing the same enemy.
   - Step 80 ticks and verify:
     - The engaged `pulse_bot` is never shoved off its firing position.
     - The `rhino_beetle` never walks on top of the `pulse_bot` (center-to-center distance remains strictly above `0.85 * (r_marine + r_beetle)`), smoothly wraps around the `pulse_bot` to reach melee range of the enemy, and exhibits $\le 2$ direction reversals (zero vibration).
3. **No Sign-Flip Vibration Across the `0.05` Off-Axis Offset (`crates/sim_core/src/steering.rs` unit test):**
   - Verify that `compute_push_displacement` for a moving unit `a` and stationary friendly unit `b` at lateral offsets `y_b - y_a = ±0.02, ±0.04, ±0.06` always pushes `b` strictly away from `a`'s centerline (`(b.pos.y - a.pos.y).signum() == d_b.y.signum()`) for both `a.id < b.id` and `a.id > b.id`, and also applies a positive forward radial component (`d_b.x > 0` when `b` is ahead of `a`).

---

## 5. Ramp-Bottom Stuck Resolution in Second Tab (`LiveBattle`)

### 5.1 Scenario Context & Exact Geometry in `LiveBattle` (`showcase_canyon.scmap.ron`)

In the second tab of the Visual Lab (`LabTab::LiveBattle`, [`crates/client/src/lab_ui.rs`](../../crates/client/src/lab_ui.rs#L653-L735)), clicking **Attack destination** issues `UnitOrder::AttackMove { target }` to the player's combat squad toward the opponent's base (`(53.0, 53.0)` for Player 0 or `(6.0, 6.0)` for Player 1):
- **Squad Loadout (`assets/scenarios/default_battle.ron` & `assets/catalog/start_loadouts.ron`):**
  - 8 `pulse_bot` (`collision_radius = 0.375`, `UnitClearance::Small`, `move_speed = 4.0`)
  - 2 `rhino_beetle` (`collision_radius = 0.5625`, `UnitClearance::Medium`, `move_speed = 3.5`)
- **Ramp Geometry in `assets/maps/showcase_canyon.scmap.ron`:**
  - **Ramp 1 (North-East base entrance):** East-ascending ramp (`EEE`) at `x = 35..=37, y = 52..=55`.
    - Bottom landing (`CliffLevel(1)`): `x = 34, y = 52..=55`.
    - Top landing (`CliffLevel(2)`): `x = 38, y = 52..=55`.
    - Low-ground side pocket directly South of the ramp wall: `x = 35..=37, y = 51` (`CliffLevel(1)`, `GROUND_PATHABLE`). The shared horizontal edge `y = 52.0` for `x ∈ [35.0, 38.0]` between `(35..=37, 51)` and the ramp cells `(35..=37, 52)` is an **impassable ramp side-wall cliff** (`terrain_step_is_connected((35, 51), (35, 52)) == false`, and diagonal `terrain_step_is_connected((34, 51), (35, 52)) == false` & `terrain_step_is_connected((35, 51), (34, 52)) == false`).
  - **Ramp 2 (South-West base entrance):** West-ascending ramp (`WWW`) at `x = 26..=28, y = 8..=11`.
    - Bottom landing (`CliffLevel(1)`): `x = 29, y = 8..=11`.
    - Top landing (`CliffLevel(2)`): `x = 25, y = 8..=11`.
    - Low-ground side pockets directly North (`x = 26..=28, y = 12`) and South (`x = 26..=28, y = 7`) of the ramp walls (`CliffLevel(1)`, `GROUND_PATHABLE`), separated from the ramp cells `(26..=28, 8..=11)` by impassable ramp side-wall edges at `y = 12.0` and `y = 8.0`.

---

### 5.2 Three Interlocking Root Causes Trapping 2 Units at the Bottom of the Ramp

#### Root Cause 5A — `offset_corner_waypoint` Ignores Ramp/Cliff Edge Disconnects & `smooth_connected_corridor` Skips the Ramp-Bottom Landing Cell (`crates/sim_map/src/pathfinding.rs` lines 716–757 & 1051–1090)

1. **Un-offset Ramp Portal Corner in `offset_corner_waypoint`:**
   - When the Simple Stupid Funnel turns around the bottom corner of a ramp—e.g. vertex `(vx, vy) = (35, 52)` at the bottom-south corner of Ramp 1 (or `(29, 12)` at the bottom-north corner of Ramp 2)—`offset_corner_waypoint` inspects the $2\times 2$ quad of cells surrounding the integer vertex `(35, 52)`:
     $$(34, 51),\quad (35, 51),\quad (34, 52),\quad (35, 52).$$
   - Because `offset_corner_waypoint` only tests `!is_cell_passable_for_request(map, request, cx, cy)` on each individual cell—and all four cells are individually walkable (`(34, 51)`, `(35, 51)`, `(34, 52)` are `CliffLevel(1)` ground; `(35, 52)` is a `RAMP` cell)—`blocked_count` is `0`!
   - Consequently, `offset_corner_waypoint` returns the **raw, un-offset integer corner `(35.0, 52.0)`** right on the impassable ramp side-wall corner!
2. **Center-Ray Shortcut in `smooth_connected_corridor`:**
   - Because the raw vertex `(35.0, 52.0)` has zero clearance from the disconnected edge between `(35, 51)` and `(35, 52)`, `segment_is_passable` rejects the funnel path for `UnitClearance::Medium` (`clearance_radius = 0.25`) or when a unit approaches near `y = 52.0`, causing `find_path_with_vision` (`pathfinding.rs` lines 404–408) to fall back to `smooth_connected_corridor`.
   - In `smooth_connected_corridor` (`pathfinding.rs` lines 716–757), line-of-sight pruning skips intermediate cell centers whenever `segment_is_passable(map, request, current, candidate_point)` succeeds.
   - When a unit is in `(34, 51)` at `(34.70, 51.97)` (just South of the ramp landing `(34, 52)`), the straight line to `(41.5, 52.5)` (past the top of the ramp) crosses `y = 52.0` at $x = 34.96 < 35.0$ (inside `(34, 52)`). For `UnitClearance::Small` (`clearance_radius = 0.0` in `segment_is_passable`), `segment_is_passable((34.70, 51.97), (41.5, 52.5))` returns `true`!
   - Thus `smooth_connected_corridor` **deletes the ramp-bottom landing waypoint `(34.5, 52.5)` completely** and aims the unit from `(34.70, 51.97)` straight across the ramp corner at `x = 35.0, y = 52.0` toward `(41.5, 52.5)`! Even a `0.04` eastward nudge from unit avoidance or `prepare_slide` puts the unit at $x > 35.0$ in `(35, 51)` directly underneath the impassable ramp side-wall!

#### Root Cause 5B — `prepare_slide` Unconditionally Prefers `X` Over `Y`, Trapping Units in an Infinite Horizontal Ping-Pong at the Ramp Corner (`crates/sim_core/src/movement/locomotion.rs` lines 525–555)

Look at `prepare_slide` in [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L525-L555):
```rust
let next = add(actor.position, velocity).map_err(|e| arithmetic(id, e))?;
if policy.permits_segment(&self.map, actor.position, next) {
    return Ok((next, velocity));
}
let x = SimVec2::new(next.x, actor.position.y);
if velocity.x != SimVec2::ZERO.x && policy.permits_segment(&self.map, actor.position, x) {
    return Ok((x, SimVec2::new(velocity.x, SimVec2::ZERO.y)));
}
let y = SimVec2::new(actor.position.x, next.y);
if velocity.y != SimVec2::ZERO.y && policy.permits_segment(&self.map, actor.position, y) {
    return Ok((y, SimVec2::new(SimVec2::ZERO.x, velocity.y)));
}
```
Trace what happens tick-by-tick when a unit approaches the bottom-south corner `(35.0, 52.0)` of Ramp 1 from `(34, 51)` at `pos = (34.98, 51.95)`:
1. **Tick $t$ (in cell `(34, 51)` at `(34.98, 51.95)`):**
   - The unit steers North-North-East toward the ramp (`velocity = (+0.03, +0.085)`).
   - Full step `next = (35.01, 52.035)` lands in ramp cell `(35, 52)`. Because `(34, 51) -> (35, 52)` is a diagonal step where `(35, 51) -> (35, 52)` is blocked by the ramp side-wall, `policy.permits_segment((34.98, 51.95), (35.01, 52.035))` returns **`false`**!
   - Both single-axis slides are valid:
     - `x = (35.01, 51.95)` slides East into the dead-end pocket `(35, 51)` underneath the ramp wall (`permits_segment == true`).
     - `y = (34.98, 52.035)` slides North into the ramp-bottom landing `(34, 52)` (`permits_segment == true`), which is the actual direction of travel (`|velocity.y| = 0.085 >> |velocity.x| = 0.03`)!
   - Because `prepare_slide` **unconditionally tests `x` before `y`**, it discards the `0.085` northward step into the ramp landing `(34, 52)` and instead slides the unit East to `(35.01, 51.95)` into `(35, 51)` underneath the ramp side-wall!
2. **Tick $t + 1$ (now in cell `(35, 51)` at `(35.01, 51.95)`):**
   - From `(35.01, 51.95)`, the unit aims North-West toward the ramp landing `(34.5, 52.5)` (`velocity = (-0.04, +0.08)`).
   - Full step `next = (34.97, 52.03)` lands in `(34, 52)`. Because `(35, 51) -> (34, 52)` is a diagonal across the ramp corner where `(35, 51) -> (35, 52)` is blocked, `policy.permits_segment((35.01, 51.95), (34.97, 52.03))` returns **`false`**!
   - The Y-slide `(35.01, 52.03)` into `(35, 52)` is blocked by the ramp wall (`permits_segment == false`), while the X-slide `(34.97, 51.95)` West back into `(34, 51)` succeeds!
3. **Tick $t + 2$ (back in cell `(34, 51)` at `(34.97, 51.95)`):**
   - Now back in `(34, 51)`, if the waypoint was popped to `(38.5, 52.5)` or local avoidance deflects `velocity.x > 0` again, `prepare_slide` once again takes the `x` branch into `(35, 51)`!
   - The 2 trailing/flank units oscillate horizontally between `(34.97, 51.95)` and `(35.01, 51.95)` at the bottom corner of the ramp forever!

#### Root Cause 5C — Local Avoidance Deflection Into the Ramp Side-Wall Pocket & Unchecked Intermediate Waypoint Popping (`crates/sim_core/src/movement/locomotion.rs` lines 350–413)

1. When the 10-unit squad funnels into the 4-cell-wide ramp entrance, `compute_unified_local_steering` (`locomotion.rs` line 396) applies a strong lateral avoidance vector (`0.85 * tangent`) away from ahead/overlapping allies in `(34, 52)`. For the 2 units on the South flank of Ramp 1 (or North/South flanks of Ramp 2), that tangent points directly into the low-ground pocket `(35, 51)` beside the ramp wall.
2. Once pushed into `(35, 51)` (e.g. `(35.2, 51.8)`), if the unit's current waypoint `next` is `(34.5, 52.5)`:
   - Note that the straight line from `(35.2, 51.8)` to `(34.5, 52.5)` crosses `x = 35.0` and `y = 52.0` at `(35.0, 52.0)`—a blocked diagonal transition (`policy.permits_segment(&self.map, actor.position, next) == false`), yet `segment_is_passable` (for `Small` zero-radius ray if it crosses `x = 35.0` at `y = 51.99 < 52.0`) can still return `true`!
   - Because `prepare_actor_motion` only checked `!self.pathfinder.segment_is_passable(&self.map, &req, actor.position, target)` when `proposal.route(actor).is_empty()`, a unit whose `first_wp` in `proposal.route(actor)` is no longer reachable via `policy.permits_segment(&self.map, actor.position, first_wp)` never re-paths around the ramp corner via `(34.5, 51.5) -> (34.5, 52.5)`!

---

### 5.3 Architectural Design & Mathematical Specification for Section 5

#### Fix 5.1 — Ramp/Cliff Corner Disconnect Offset in `offset_corner_waypoint` & Ramp-Portal Preservation in `smooth_connected_corridor` (`crates/sim_map/src/pathfinding.rs`)

1. **Detect Disconnected Ramp/Cliff Edges in `offset_corner_waypoint` (`pathfinding.rs` lines 1051–1090):**
   - In `offset_corner_waypoint`, when `blocked_count == 0` (all four cells `(vx - 1, vy - 1)`, `(vx, vy - 1)`, `(vx - 1, vy)`, `(vx, vy)` around vertex `(vx, vy)` are individually passable), check whether any of the 4 orthogonal edges between those 4 cells is disconnected (`!map.terrain_step_is_connected(a, b)`):
     - Let the 4 quadrant cells be $Q_{00} = (vx - 1, vy - 1)$, $Q_{10} = (vx, vy - 1)$, $Q_{01} = (vx - 1, vy)$, $Q_{11} = (vx, vy)$.
     - A quadrant cell $Q_{ij}$ is a **valid corner pivot** if and only if it is connected via `map.terrain_step_is_connected` to **both** of its orthogonal neighbors in the $2\times 2$ quad!
       - Specifically, at the ramp corner `(35, 52)`:
         - $Q_{00} = (34, 51)$ is connected to $Q_{10} = (35, 51)$ and $Q_{01} = (34, 52)$ ✅
         - $Q_{01} = (34, 52)$ is connected to $Q_{00} = (34, 51)$ and $Q_{11} = (35, 52)$ ✅
         - $Q_{10} = (35, 51)$ is **not** connected to $Q_{11} = (35, 52)$ ❌ (ramp side-wall!)
         - $Q_{11} = (35, 52)$ is **not** connected to $Q_{10} = (35, 51)$ ❌ (ramp side-wall!)
     - Therefore, when `blocked_count == 0` but `!map.terrain_step_is_connected((vx - 1, vy - 1), (vx, vy))` (which by `GridMap::terrain_step_is_connected`'s diagonal definition is `true` whenever *any* of the 4 orthogonal edges in the $2\times 2$ quad is disconnected!), we treat any quadrant cell $Q_{ij}$ that is disconnected from either of its two orthogonal quad neighbors as **blocked across the corner** and push `(ox, oy)` away from the disconnected edge!
     - At `(35, 52)`, $Q_{10} = (35, 51)$ and $Q_{11} = (35, 52)$ lack vertical connectivity across `y = 52.0`, so both have `dx = +1` (East of `x = 35.0`). Treating non-pivot cells as blocked yields `ox = -2, oy = 0`, which offsets the waypoint West by `clearance_radius` (or a minimum `0.25` corner margin when `clearance_radius == 0`) into `x < 35.0` (the connected landing column `x = 34`)!
     - Even better: if `clearance_radius` is `0` (`UnitClearance::Small`), a disconnected cliff/ramp corner still requires a non-zero offset so the waypoint does not lie on the integer boundary `x = 35.0, y = 52.0` where fixed-point rounding or slide touches `(35, 51)`. Using `let effective_offset = request.clearance.clearance_radius().max(SimScalar::from_Ratio(1, 4))` (`0.25`) when `blocked_count == 0 && !map.terrain_step_is_connected((vx - 1, vy - 1), (vx, vy))` guarantees that even `Small` units receive a waypoint at `(34.75, 52.0)` safely inside the connected ramp-landing column `x = 34`!

2. **Preserve Ramp-Landing Portal Cells in `smooth_connected_corridor` (`pathfinding.rs` lines 716–757):**
   - In `smooth_connected_corridor`, before skipping intermediate cells between `anchor_index` and `candidate_index`, check if any intermediate cell `corridor[k]` (`anchor_index < k < candidate_index`) is a **ramp transition cell** (i.e. `map.is_ramp(corridor[k].0, corridor[k].1) != map.is_ramp(corridor[k - 1].0, corridor[k - 1].1)` or `map.is_ramp(corridor[k].0, corridor[k].1) != map.is_ramp(corridor[k + 1].0, corridor[k + 1].1)`).
   - If `corridor[k]` is a ramp-boundary transition cell, only allow skipping `corridor[k]` when `segment_is_passable` also holds with at least `UnitClearance::Medium` (`0.25` margin) AND `map.permits_ground_segment(current, candidate_point)` holds for `current` offset by `±0.25` on both axes! Simplest, cleanest, and 100% textbook invariant: **never skip a cell in `smooth_connected_corridor` where the corridor transitions between non-ramp and ramp terrain (`map.is_ramp(prev) != map.is_ramp(curr)` or `map.is_ramp(curr) != map.is_ramp(next)`) unless `current` and `candidate_point` share the ramp's entry coordinate lane**. Keeping the ramp-mouth landing cell center (e.g. `(34.5, 52.5)`) in `smooth_connected_corridor` guarantees that units approaching from South of the ramp (`y < 52`) always align with the ramp mouth at `(34.5, 52.5)` before ascending the ramp!

#### Fix 5.2 — Velocity-Dominant Slide Priority in `prepare_slide` (`crates/sim_core/src/movement/locomotion.rs` lines 525–555)

In `prepare_slide` (`crates/sim_core/src/movement/locomotion.rs` lines 525–555):
- When the full step `next = add(actor.position, velocity)` is rejected by `policy.permits_segment(&self.map, actor.position, next)`, evaluate both candidate axis slides:
  - `x_cand = SimVec2::new(next.x, actor.position.y)` (`x_ok = velocity.x != SimScalar::ZERO && policy.permits_segment(&self.map, actor.position, x_cand)`)
  - `y_cand = SimVec2::new(actor.position.x, next.y)` (`y_ok = velocity.y != SimScalar::ZERO && policy.permits_segment(&self.map, actor.position, y_cand)`)
- When **both** `x_ok` and `y_ok` are true, choose the axis with the larger velocity magnitude:
  - If `velocity.y.abs() > velocity.x.abs()`, return `(y_cand, SimVec2::new(SimScalar::ZERO, velocity.y))`!
  - Otherwise return `(x_cand, SimVec2::new(velocity.x, SimScalar::ZERO))`.
- Why this immediately breaks the ramp-corner trap:
  - When a unit at `(34.98, 51.95)` in `(34, 51)` steers North-North-East (`velocity = (+0.03, +0.085)`) toward the ramp landing `(34, 52)`, `|velocity.y| = 0.085 > |velocity.x| = 0.03`.
  - Instead of sliding East into the dead-end pocket `(35, 51)` underneath the ramp wall, `prepare_slide` slides North to `(34.98, 52.035)` directly into the ramp-bottom landing cell `(34, 52)`!

#### Fix 5.3 — Guard Against Local-Avoidance Deflection Across Impassable Ramp Corners & Re-Path Disconnected First Waypoint (`crates/sim_core/src/movement/locomotion.rs`)

1. **Re-Path When `first_wp` Is Disconnected From `actor.position` (`locomotion.rs` lines 330–355):**
   - If a unit has already been pushed into `(35, 51)` (e.g. `(35.2, 51.8)`) while its current `first_wp` is `(34.5, 52.5)` or `(38.5, 52.5)`, the direct segment from `actor.position` to `first_wp` crosses the disconnected ramp corner (`!policy.permits_segment(&self.map, actor.position, first_wp)` or `!self.pathfinder.segment_is_passable(&self.map, &req, actor.position, first_wp)`).
   - Adding the check in `prepare_actor_motion`:
     ```rust
     if let Some(&first_wp) = proposal.route(actor).first()
         && policy.layer == MovementLayer::Ground
         && (!policy.permits_segment(&self.map, actor.position, first_wp)
             || !self
                 .pathfinder
                 .segment_is_passable(&self.map, &req, actor.position, first_wp))
     {
         if let Ok(path) = self.pathfinder.find_path_with_vision(&self.map, &req) {
             proposal.replacement = Some(path.waypoints);
             proposal.remove_prefix = 0;
         }
     }
     ```
     immediately replaces the blocked route with `(34.5, 51.5) -> (34.5, 52.5) -> ...`, guiding the unit West out of `(35, 51)` into `(34, 51)` and then North into the ramp landing `(34, 52)`!
2. **Prevent Local Avoidance From Deflecting a Unit Into a Cell Disconnected From `next` (`locomotion.rs` lines 396–413):**
   - In `prepare_actor_motion`, after computing `direction = compute_unified_local_steering(...)` and `velocity = scale(direction, step)`:
   - Check if `policy.layer == MovementLayer::Ground` and the steered step `steered_pos = add(actor.position, velocity)` is valid (`policy.permits_segment(&self.map, actor.position, steered_pos)`), **but** from `steered_pos` the current waypoint `next` would become terrain-disconnected (`!policy.permits_segment(&self.map, steered_pos, next)`) whereas from `actor.position` `next` **was** terrain-connected (`policy.permits_segment(&self.map, actor.position, next)`):
   - In that exact case (local avoidance is about to shove the unit across `x = 35.0` from `(34, 51)` into the dead-end pocket `(35, 51)` from which `next = (34.5, 52.5)` is blocked by the ramp corner!), fall back to the direct un-deflected goal direction `goal_dir = direction(next, actor.position)` for `velocity` before calling `prepare_slide`!

---

### 5.4 Step-by-Step Code Changes for Section 5

#### Step 4: `crates/sim_map/src/pathfinding.rs`

1. **Update `offset_corner_waypoint` (`pathfinding.rs` lines 1051–1090) to Offset Disconnected Ramp/Cliff Portal Corners:**
   ```rust
   fn offset_corner_waypoint(
       map: &GridMap,
       request: &PathRequestWithVision<'_>,
       vx: usize,
       vy: usize,
   ) -> SimVec2 {
       let base_x = SimScalar::from_num(vx);
       let base_y = SimScalar::from_num(vy);
       let clearance = request.clearance.clearance_radius();

       let q00 = (vx.wrapping_sub(1), vy.wrapping_sub(1));
       let q10 = (vx, vy.wrapping_sub(1));
       let q01 = (vx.wrapping_sub(1), vy);
       let q11 = (vx, vy);

       let quadrants: [((usize, usize), i32, i32); 4] = [
           (q00, -1, -1),
           (q10, 1, -1),
           (q01, -1, 1),
           (q11, 1, 1),
       ];

       let mut ox: i32 = 0;
       let mut oy: i32 = 0;
       let mut blocked_count: u32 = 0;

       for &((cx, cy), dx, dy) in &quadrants {
           if !is_cell_passable_for_request(map, request, cx, cy) {
               ox -= dx;
               oy -= dy;
               blocked_count += 1;
           }
       }

       // If all 4 cells around `(vx, vy)` are individually passable, check whether a
       // cliff or ramp side-wall disconnects any orthogonal edge in the 2x2 quad.
       let mut effective_clearance = clearance;
       if blocked_count == 0
           && vx > 0
           && vy > 0
           && !map.terrain_step_is_connected(q00, q11)
       {
           let ortho_neighbors: [((usize, usize), [(usize, usize); 2], i32, i32); 4] = [
               (q00, [q10, q01], -1, -1),
               (q10, [q00, q11], 1, -1),
               (q01, [q00, q11], -1, 1),
               (q11, [q10, q01], 1, 1),
           ];
           for &(cell, neighbors, dx, dy) in &ortho_neighbors {
               if !neighbors
                   .iter()
                   .all(|&nbr| map.terrain_step_is_connected(cell, nbr))
               {
                   ox -= dx;
                   oy -= dy;
                   blocked_count += 1;
               }
           }
           let min_corner_margin = SimScalar::from_f32_lossy(0.25);
           if effective_clearance < min_corner_margin {
               effective_clearance = min_corner_margin;
           }
       }

       if effective_clearance <= SimScalar::ZERO || blocked_count == 0 || blocked_count == 4 {
           return SimVec2::new(base_x, base_y);
       }

       let offset_x = if ox > 0 {
           effective_clearance
       } else if ox < 0 {
           -effective_clearance
       } else {
           SimScalar::ZERO
       };
       let offset_y = if oy > 0 {
           effective_clearance
       } else if oy < 0 {
           -effective_clearance
       } else {
           SimScalar::ZERO
       };

       SimVec2::new(base_x + offset_x, base_y + offset_y)
   }
   ```

2. **Preserve Ramp-Portal Transition Cells in `smooth_connected_corridor` (`pathfinding.rs` lines 716–757):**
   - Inside `smooth_connected_corridor`, before testing `segment_is_passable(map, request, current, candidate_point)` for `candidate_index > anchor_index + 1`:
   - Check whether any skipped intermediate cell `corridor[k]` (`anchor_index < k < candidate_index`) is a ramp transition cell (`map.is_ramp(corridor[k].0, corridor[k].1) != map.is_ramp(corridor[k - 1].0, corridor[k - 1].1)` or `map.is_ramp(corridor[k].0, corridor[k].1) != map.is_ramp(corridor[k + 1].0, corridor[k + 1].1)`). If so, do not skip past `k` unless `corridor[anchor_index]` and `corridor[candidate_index]` are collinear along the ramp's axis (`corridor[anchor_index].0 == corridor[candidate_index].0` or `corridor[anchor_index].1 == corridor[candidate_index].1`):
   ```rust
   let skips_ramp_turn = ((anchor_index + 1)..candidate_index).any(|k| {
       let prev = corridor[k - 1];
       let curr = corridor[k];
       let next = corridor[k + 1];
       let ramp_transition = map.is_ramp(curr.0, curr.1) != map.is_ramp(prev.0, prev.1)
           || map.is_ramp(curr.0, curr.1) != map.is_ramp(next.0, next.1);
       let collinear = corridor[anchor_index].0 == corridor[candidate_index].0
           || corridor[anchor_index].1 == corridor[candidate_index].1;
       ramp_transition && !collinear
   });
   if !skips_ramp_turn && segment_is_passable(map, request, current, candidate_point) {
       next_index = candidate_index;
       next_point = candidate_point;
       break;
   }
   ```

#### Step 5: `crates/sim_core/src/movement/locomotion.rs`

1. **Velocity-Dominant Axis Priority in `prepare_slide` (`locomotion.rs` lines 525–555):**
   ```rust
   fn prepare_slide(
       &self,
       id: EntityId,
       actor: &ActorSnapshot,
       policy: ActorMovementPolicy,
       velocity: SimVec2,
   ) -> Result<(SimVec2, SimVec2), MovementInvariant> {
       let next = add(actor.position, velocity).map_err(|e| arithmetic(id, e))?;
       if policy.permits_segment(&self.map, actor.position, next) {
           return Ok((next, velocity));
       }
       let x = SimVec2::new(next.x, actor.position.y);
       let x_ok =
           velocity.x != SimVec2::ZERO.x && policy.permits_segment(&self.map, actor.position, x);
       let y = SimVec2::new(actor.position.x, next.y);
       let y_ok =
           velocity.y != SimVec2::ZERO.y && policy.permits_segment(&self.map, actor.position, y);
       if x_ok && y_ok {
           if velocity.y.abs() > velocity.x.abs() {
               return Ok((y, SimVec2::new(SimVec2::ZERO.x, velocity.y)));
           }
           return Ok((x, SimVec2::new(velocity.x, SimVec2::ZERO.y)));
       }
       if x_ok {
           return Ok((x, SimVec2::new(velocity.x, SimVec2::ZERO.y)));
       }
       if y_ok {
           return Ok((y, SimVec2::new(SimVec2::ZERO.x, velocity.y)));
       }
       Ok((actor.position, SimVec2::ZERO))
   }
   ```

2. **Prevent Local Avoidance From Deflecting a Unit Into a Terrain-Disconnected Cell (`locomotion.rs` lines 396–413):**
   ```rust
   let mut velocity = scale(direction, step).map_err(|e| arithmetic(id, e))?;
   if policy.layer == MovementLayer::Ground
       && let Ok(steered_pos) = add(actor.position, velocity)
       && policy.permits_segment(&self.map, actor.position, steered_pos)
       && policy.permits_segment(&self.map, actor.position, next)
       && !policy.permits_segment(&self.map, steered_pos, next)
   {
       let direct_dir =
           super::math::direction(next, actor.position).map_err(|e| arithmetic(id, e))?;
       velocity = scale(direct_dir, step).map_err(|e| arithmetic(id, e))?;
   }
   let (position, velocity) = self.prepare_slide(id, actor, policy, velocity)?;
   ```

---

### 5.5 Verification Test for Section 5

Add a deterministic integration test in `crates/sim_core/tests/collision_and_immobility_tests.rs` (and/or `crates/sim_map/tests/funnel_tests.rs`):
1. **Full 10-Unit `LiveBattle` Squad Ramp Traversal Test (`showcase_canyon.scmap.ron` Ramp 1 & Ramp 2 Geometry):**
   - Construct the exact `showcase_canyon.scmap.ron` Ramp 1 (`EEE` at `x = 35..=37, y = 52..=55`, bottom landing `x = 34`, top landing `x = 38`, low-ground pocket `x = 35..=37, y = 51` on `CliffLevel(1)`) and Ramp 2 (`WWW` at `x = 26..=28, y = 8..=11`) geometry, or load `showcase_canyon.scmap.ron` directly.
   - Spawn the 10-unit `battle_automaton` squad (8 `pulse_bot` + 2 `rhino_beetle`) and issue `AttackMove` / `Move` across the map from `(6, 6)` to `(53, 53)` (and vice versa).
   - Step the simulation and assert that **all 10 units** cleanly ascend the ramp onto `CliffLevel(2)` (`x >= 38` for Ramp 1; `x <= 25` for Ramp 2) with **zero units stuck** in `(35..=37, 51)` or `(26..=28, 12)` at the bottom of the ramp!

### Mandatory Quality Gate
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check`

