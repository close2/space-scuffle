# Stage 1 Architectural Blueprint: Default Smog Parameters (`5.0` / `3.4`), 200-Unit Swarm Mist Volume Alignment, and Smooth Bubble Trajectories

**Date:** 2026-10-30  
**Status:** Approved Stage 1 Blueprint  
**Target Crates:** `crates/client`, `crates/sim_map`, `crates/sim_core`

---

## 1. Executive Summary & Problem Statement

The user observed three issues during interactive exploration and battlefield simulation:

1. **Unified Default Smog Parameters (`emission_rate = 5.0`, `wake_influence = 3.4`):**
   - All bubbles across `VisualLab`, `LiveBattle`, and `SwarmStressTest` should default to `emission_rate = 5.0` and `wake_influence = 3.4` without hidden per-tab multipliers.
2. **200-Unit Swarm Stress Test (`LabTab::SwarmStressTest`) Shows Zero Mist:**
   - Navigating to the 200-unit Swarm tab shows no mist for two compounding reasons:
     1. `LabState::default().swarm_active` is initialized to `false` ([`crates/client/src/lab_ui.rs`](../../crates/client/src/lab_ui.rs#L165)), so switching to the `Swarm` tab in the interactive client spawns 0 bubbles unless `--swarm` was passed on the CLI or the user manually clicks `"Spawn 200-Bubble Swarm"`.
     2. Even when the 200 `SwarmBubble` entities are active, `update_smog_field_system` ([`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs#L675-L718)) only distinguishes `if current_tab == LabTab::VisualLab` vs. `else` (which assumes `LiveBattle`). In `SwarmStressTest`:
        - The 200 `SwarmBubble` entities are spawned at `x ∈ [6.0, 26.0]`, `y = 0.5` (`ground_z = 0.0`, `radius = 0.45`), and `z ∈ [10.0, 20.8]` ([`crates/client/src/scene_controller.rs`](../../crates/client/src/scene_controller.rs#L783-L798)).
        - Because `SwarmBubble` has `ground_z = 0.0 <= 0.1` and `RtsCamera::default()` has `look_at_y = 1.2` (`cam_ground_y = 0.0 <= 0.1`), `update_smog_field_system` falls back to `ground_y = 3.0_f32` (`target_min.y = 3.0, target_max.y = 5.8`) and centers Z around `RtsCamera::default().target.y = 32.0` (`target_min.z = 21.0, target_max.z = 43.0`).
        - Meanwhile, `SwarmBubble` has `is_live_battle_unit == false`, so its source position remains at `y = 0.5` and `z ∈ [10.0, 20.8]`—**100% outside** the 3D fluid volume `(2.0..30.0, 3.0..5.8, 21.0..43.0)`! Every single one of the 200 bubbles is culled by the AABB check at [`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs#L801-L805), yielding `active_source_count == 0` and `total_density == 0.0`.
3. **Bubble Paths in the Battlefield Simulation Are Not Smooth (Frequent Small Adjustments / Micro-Swerves):**
   - Tracing individual bubble trajectories across `showcase_canyon.scmap.ron` (`default_battle.ron`) reveals four distinct deterministic simulation mechanisms that inject high-frequency angular adjustments, sawtooth oscillations, and staircase waypoints into unit locomotion:
     - **Mechanism 3A (`compute_unified_local_steering` in [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L161-L223)):**
       1. *Co-moving squadmates treated as static walls:* `SteeringObstacle` already carries `pub velocity: SimVec2`, yet `compute_unified_local_steering` ignores `obs.velocity`. In `LiveBattle`, 8 squadmates per side march in the same direction (`dot(obs.velocity, goal_dir) > 0`). Whenever a squadmate ahead or alongside is within `r_1 + r_2 + 0.35 = 1.10 m`, the trailing bubble swerves $40.4^\circ$ away from a teammate that is already moving forward at the same speed.
       2. *Binary $40.4^\circ$ step-function swerve:* Entering `threshold = r_1 + r_2 + lookahead` jumps steering discontinuously from `goal_dir` ($0^\circ$) to `normalize(goal_dir + 0.85 * tangent)` ($40.36^\circ$). One tick later, stepping $0.01\text{ m}$ outside `threshold` snaps $-40.36^\circ$ back to `goal_dir`, creating a tick-by-tick sawtooth (`0° -> 40.4° -> 0° -> 40.4°`).
       3. *Single `closest_blocker` left-right flip-flopping:* Only a single `closest_blocker` is tracked. When walking between two neighbors on the left and right at similar distances, `closest_blocker` alternates between them on consecutive ticks, snapping steering $+40.4^\circ$ right on tick $t$ and $-40.4^\circ$ left on tick $t+1$.
       4. *Lateral corridor over-triggering & `0.05` deadzone sign flip:* `within_distance` triggers avoidance even when `|lat| >= r_1 + r_2` (where straight-line travel along `goal_dir` already misses `obs`), and the `0.05` cross-product deadzone (`line 205`) flips the tangent sign when `cross` crosses $+0.05$.
     - **Mechanism 3B (Co-Moving Wedge Push Fighting Locomotion in [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L491-L521)):**
       - In `checked_dynamic_push`, when two co-moving friendly units overlap (`a_moving && b_moving`), lines 499–517 take `move_dir` solely from `a.vel` and apply a discontinuous `0.25 * perp` wedge whenever `|dot(separation, move_dir)| > 0.5`. At the end of tick $t$, `resolve_spatial_overlap_pushes` nudges `actor.position` sideways/diagonally; on tick $t+1$, `prepare_actor_motion` steers back toward the waypoint while `checked_dynamic_push` wedges it out again, producing a 2-tick positional micro-sawtooth.
     - **Mechanism 3C (`offset_corner_waypoint` Places Ramp Portal Corners on Integer Grid Lines in [`crates/sim_map/src/pathfinding.rs`](../../crates/sim_map/src/pathfinding.rs#L1146-L1182)):**
       - At a ramp portal corner such as `(35, 52)` (Ramp 1 bottom-south corner) or `(29, 12)` (Ramp 2 bottom-north corner), two cells in the $2\times 2$ vertex quad fail orthogonal connectivity (`q10=(35,51)` and `q11=(35,52)` at `(35,52)`; `q00=(28,11)` and `q01=(28,12)` at `(29,12)`).
       - Because their `away_y` offsets are `+1` and `-1`, `dir_y` sums to **`0`**!
       - At `(29, 12)` (Ramp 2, `y = 8..=11`), `offset_corner_waypoint` returns `(29.425, 12.0)`. Because `world_to_cell` uses `.floor()`, `y = 12.0` maps to row `y = 12` (**outside the ramp** in the cliff wall row `y = 12` instead of row `y = 11` inside the ramp)!
       - Consequently, `waypoints.windows(2).all(|pair| self.segment_is_passable(...))` (`pathfinding.rs` line 639) **rejects the smooth string-pulled funnel path** across Ramp 2 and falls back to `smooth_connected_corridor` (staircase cell-center waypoints), forcing units to make discrete $45^\circ$ / $90^\circ$ cell-to-cell adjustments!
     - **Mechanism 3D (Per-Tick Full-Chain `segment_is_passable` Route Wiping in [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L314-L371)):**
       - In `prepare_actor_motion`, `actor.current_funnel_waypoints.iter().all(|&point| self.pathfinder.segment_is_passable(&self.map, &req, previous, point))` re-validates every segment of the remaining route on every tick, and if `actor.position` is nudged by $0.05\text{ m}$ near a ramp corner so that the chord `(actor.position, first_wp)` grazes an integer grid boundary, lines 323–325 wipe `actor.current_funnel_waypoints` (`proposal.clear_route = true`) and re-run A* from scratch on every tick.

---

## 2. Detailed Architectural Design

### 2.1 Part 1: Default `emission_rate = 5.0` and `wake_influence = 3.4` for All Bubbles

1. **`SmogConfig::default()` in [`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs#L94-L108):**
   - Update `emission_rate: 4.4` $\rightarrow$ `emission_rate: 5.0`.
   - Update `wake_influence: 4.2` $\rightarrow$ `wake_influence: 3.4`.
2. **`LabState::default()` in [`crates/client/src/lab_ui.rs`](../../crates/client/src/lab_ui.rs#L169-L170):**
   - Update `smog_emission_rate: 4.4` $\rightarrow$ `smog_emission_rate: 5.0`.
   - Update `smog_wake_influence: 4.2` $\rightarrow$ `smog_wake_influence: 3.4`.
3. **Uniform Bubble Source Strengths in `update_smog_field_system` ([`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs#L810-L814)):**
   - Replace the per-tab multiplier:
     ```rust
     let (emission_strength, influence_strength) = (1.0_f32, 1.0_f32);
     ```
     so that `effective_config.emission_rate` (`5.0`) and `effective_config.wake_influence` (`3.4`) apply identically to all bubbles in `VisualLab`, `LiveBattle`, and `SwarmStressTest`.

---

### 2.2 Part 2: Enabling & Aligning the 3D Smog Field in the 200-Unit Swarm Stress Test (`LabTab::SwarmStressTest`)

1. **Default `swarm_active: true` in `LabState::default()` ([`crates/client/src/lab_ui.rs`](../../crates/client/src/lab_ui.rs#L165)):**
   - Set `swarm_active: true` in `LabState::default()`.
   - Because `sync_swarm_stress_system` ([`crates/client/src/scene_controller.rs`](../../crates/client/src/scene_controller.rs#L650)) guards spawning behind `if lab_state.current_tab != LabTab::SwarmStressTest || !lab_state.swarm_active`, setting `swarm_active: true` has zero cost in `VisualLab` or `LiveBattle`, and guarantees that clicking the `"3. 200-Unit Swarm"` tab immediately spawns and animates the 200-bubble swarm.
2. **Explicit `LabTab::SwarmStressTest` Volume Bounds & Pre-Warm in `update_smog_field_system` ([`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs#L675-L855)):**
   - In `sync_swarm_stress_system` (`scene_controller.rs` lines 783–798 & 863–877), the 200 `SwarmBubble` entities oscillate horizontally across `x ∈ [6.0, 26.0]`, sit at height `y = 0.5` (`radius = 0.45`), and occupy 10 rows across `z ∈ [10.0, 20.8]` (centroid `(16.0, 0.5, 15.4)`).
   - In `update_smog_field_system`, match explicitly on `current_tab`:
     ```rust
     let (target_min, target_max) = match current_tab {
         crate::LabTab::VisualLab => (Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5)),
         crate::LabTab::SwarmStressTest => (Vec3::new(4.0, 0.0, 7.0), Vec3::new(28.0, 2.8, 24.0)),
         crate::LabTab::LiveBattle => {
             // Existing camera-following / unit-centroid LiveBattle bounds
             ...
         }
     };
     ```
   - With `target_min = (4.0, 0.0, 7.0)` and `target_max = (28.0, 2.8, 24.0)` ($24\text{ m} \times 2.8\text{ m} \times 17\text{ m}$), all 200 `SwarmBubble` entities (`x ∈ [6.0, 26.0], y = 0.5, z ∈ [10.0, 20.8]`) lie well inside the 3D fluid grid with a $\ge 2.0\text{ m}$ margin on all sides for wake advection and vortex shedding.
   - Extend the first-frame 15-substep trajectory pre-warm (`needs_prewarm` at `smog/mod.rs` line 774) from `current_tab == crate::LabTab::LiveBattle` to `matches!(current_tab, crate::LabTab::LiveBattle | crate::LabTab::SwarmStressTest)` so that switching to `SwarmStressTest` (or running a 1-frame capture) immediately shows developed mist wakes behind all 200 moving bubbles.
3. **Auto-Frame Camera When Switching to `LabTab::SwarmStressTest` ([`crates/client/src/scene_controller.rs`](../../crates/client/src/scene_controller.rs#L649-L780)):**
   - When `sync_swarm_stress_system` spawns the interactive 200-bubble swarm (`controlled_fixture.is_none()`), also frame `RtsCamera` (if present) onto the swarm center `target = Vec2::new(16.0, 15.4)`, `look_at_y = 0.5`, `distance = 22.0` so that switching from `VisualLab` or `LiveBattle` to `SwarmStressTest` centers the 200 bubbles and their 3D smog volume directly in the camera frustum.
   - Also attach `crate::mist::MistMotion { conveyor_velocity: None, enabled: false }` in the `controlled_fixture.is_none()` spawn branch so `MistWakePlugin` does not allocate legacy 2D ribbon slots for the 200 smog-driven bubbles (while keeping `controlled_fixture.is_some()` untouched for `shell_order_audit` tests).

---

### 2.3 Part 3: Smooth Bubble Trajectories in `sim_map` and `sim_core`

#### 2.3.1 Mechanism 3A Fix: Smooth, Co-Moving-Aware, Multi-Blocker Local Avoidance (`crates/sim_core/src/steering.rs`)

In [`compute_unified_local_steering`](../../crates/sim_core/src/steering.rs#L161-L223):

1. **Preserve Exact Fixed-Point Validation & Extreme-Coordinate Invariants:**
   - First, iterate over `obstacles` to validate `if obs.radius <= SimScalar::ZERO { return Err(SimCoreError::NonPositiveRadius { radius: obs.radius }); }` for all obstacles before any filtering.
   - Keep `lookahead = SimScalar::from_f32_lossy(0.35)`, `combined = checked_scalar_add(unit_radius, obs.radius)?`, and `threshold = i128::from(combined.raw()) + i128::from(lookahead.raw())`.
   - Keep `within_distance(unit_pos, obs.pos, threshold)` and `forward = difference_product(goal_dir, obs.pos, unit_pos, false)?` (`forward <= SimScalar::ZERO` skipped).
   - If `checked_vec_sub(obs.pos, unit_pos)` returns `Err` (extreme `i64::MIN / 2` coordinates tested by `public_steering_handles_extreme_directions_and_wide_obstacle_thresholds` and `legacy_lookahead_and_arrival_raw_thresholds_keep_boundary_branch`), fall back to the existing `difference_product` cross-product branch so all synthetic boundary tests behave identically.
2. **Co-Moving Squadmate Filter (Eliminates Marching Squad Self-Swerving):**
   - When `rel = checked_vec_sub(obs.pos, unit_pos)` succeeds, check whether the obstacle is already moving forward in the same direction (`obs_forward = difference_product(goal_dir, obs.velocity, SimVec2::ZERO, false)?`).
   - Compute `overlap_threshold = i128::from(combined.raw())`.
   - If `obs_forward > SimScalar::ZERO` and `!within_distance(unit_pos, obs.pos, overlap_threshold)` (the neighbor is moving in the same hemisphere as `goal_dir` and is not physically overlapping the unit), **skip local avoidance swerving for `obs`**! Co-moving squadmates march smoothly side-by-side and in columns without triggering $40.4^\circ$ swerves around each other.
3. **Lateral Corridor Clearance Check (Eliminates Swerving Around Already-Cleared Obstacles):**
   - Define the unit-length left normal `left_normal = SimVec2::new(checked_scalar_neg(goal_dir.y)?, goal_dir.x)`.
   - Compute the signed lateral offset of the obstacle center relative to the unit's forward ray:
     $$\text{lat} = \text{rel} \cdot \text{left\_normal}$$
     where $\text{lat} > 0$ means the obstacle center lies to the left of `goal_dir`, and $\text{lat} < 0$ means to the right.
   - Define `corridor_half_width = checked_scalar_add(combined, SimScalar::from_f32_lossy(0.05))?`.
   - If `lat.abs() >= corridor_half_width`, the unit's straight-line path along `goal_dir` already clears `obs` laterally—**skip `obs`**!
4. **Zero-Deadzone Side Selection & Smooth Multi-Blocker Weighted Tangent Sum:**
   - Remove the `0.05` deadzone that caused tangent sign flips when `lat` crossed `+0.05`. Instead:
     - If `lat < SimScalar::ZERO` (obstacle is to the right of `goal_dir`), steer left (`away = left_normal`).
     - If `lat > SimScalar::ZERO` (obstacle is to the left of `goal_dir`), steer right (`away = -left_normal`).
     - Only when `lat == SimScalar::ZERO` (exact colinearity), break ties deterministically with `if unit_id.0 < obs.id.0 { left_normal } else { -left_normal }`.
   - Compute smooth radial and lateral weights in `SimScalar`:
     - Let `dist = rel.length()`.
     - `radial_penetration = (combined + lookahead - dist).max(SimScalar::ZERO)`.
     - `radial_t = checked_div(radial_penetration, lookahead)?.clamp(SimScalar::from_f32_lossy(0.15), SimScalar::ONE)`.
     - `lateral_penetration = (corridor_half_width - lat.abs()).max(SimScalar::ZERO)`.
     - `lateral_t = checked_div(lateral_penetration, corridor_half_width)?.clamp(SimScalar::from_f32_lossy(0.15), SimScalar::ONE)`.
     - `weight = checked_mul(SimScalar::from_f32_lossy(0.65), checked_mul(radial_t, lateral_t)?)?`.
   - Accumulate `tangent_sum = checked_vec_add(tangent_sum, checked_vec_scale(away, weight)?)?` across **all** qualifying blockers rather than picking only a single `closest_blocker`.
   - Why this makes paths smooth:
     - As a bubble approaches a stationary obstacle, `radial_t * lateral_t` ramps smoothly from `0.0` at the outer boundary up to `0.65` at contact and smoothly back down to `0.0` as `lat.abs()` reaches `corridor_half_width`, completely eliminating the $40.4^\circ$ binary sawtooth.
     - When walking between two obstacles on the left and right, their `away` vectors point in opposite directions and smoothly balance instead of flip-flopping $+40.4^\circ / -40.4^\circ$ on alternating ticks.

#### 2.3.2 Mechanism 3B Fix: Smooth Radial Separation for Co-Moving Friendly Pairs (`crates/sim_core/src/steering.rs`)

In [`checked_dynamic_push`](../../crates/sim_core/src/steering.rs#L491-L521):
- Currently, whenever `a_moving || b_moving` and `a.priority == b.priority`, `checked_dynamic_push` adds a discontinuous `0.25 * perp` wedge whenever `|dot(separation, move_dir)| > 0.5`.
- When **both** units are moving (`a_moving && b_moving`) in the same general direction (`checked_dot(a.vel, b.vel)? > SimScalar::ZERO`), they are co-moving squadmates rather than a moving unit trying to wedge around a stationary blocker. Applying a sideways/axial wedge in `resolve_spatial_overlap_pushes` fights their forward velocity on every tick.
- **Fix:** Apply the `0.25 * perp` wedge only when **one unit is moving and the other is stationary** (`a_moving != b_moving`) or when the two moving units are moving in **opposing directions** (`checked_dot(a.vel, b.vel)? <= SimScalar::ZERO`). When both units are co-moving (`a_moving && b_moving && checked_dot(a.vel, b.vel)? > SimScalar::ZERO`), use pure smooth radial separation (`correction = checked_vec_scale(separation, half)?`).

#### 2.3.3 Mechanism 3C Fix: Ramp-Portal Interior Offset in `offset_corner_waypoint` (`crates/sim_map/src/pathfinding.rs`)

In [`offset_corner_waypoint`](../../crates/sim_map/src/pathfinding.rs#L1146-L1182):
- Consider any interior grid vertex `(vx, vy)` where `blocked_count == 0` (all four quad cells `q00=(vx-1, vy-1)`, `q10=(vx, vy-1)`, `q01=(vx-1, vy)`, `q11=(vx, vy)` are passable by footprint) and `!map.terrain_step_is_connected(q00, q11)` (there is a cliff edge or ramp boundary meeting at `(vx, vy)`).
- Each cell `c` in the $2\times 2$ quad has two orthogonal neighbors in the quad (`orth_neighbors(c)`), and an interior direction `(into_x, into_y)` from `(vx, vy)` into `c`'s interior:
  - `q00 = (vx - 1, vy - 1)` $\rightarrow$ `into = (-1, -1)`, `orth = [q10, q01]`
  - `q10 = (vx, vy - 1)` $\rightarrow$ `into = (+1, -1)`, `orth = [q00, q11]`
  - `q01 = (vx - 1, vy)` $\rightarrow$ `into = (-1, +1)`, `orth = [q00, q11]`
  - `q11 = (vx, vy)` $\rightarrow$ `into = (+1, +1)`, `orth = [q10, q01]`
- Define a quad cell `c` as a **connected pivot cell** if it is terrain-connected to **both** of its orthogonal quad neighbors:
  $$\text{is\_pivot}(c) \iff \forall n \in \text{orth}(c),\ \text{map.terrain\_step\_is\_connected}(c, n)$$
- Define a connected pivot cell `c` as a **ramp-portal pivot cell** if at least one of its orthogonal quad neighbors has `CellFlags::RAMP`:
  $$\text{is\_ramp\_pivot}(c) \iff \text{is\_pivot}(c) \land \exists n \in \text{orth}(c),\ \text{map.cell}(n).\text{flags.contains}(\text{RAMP})$$
- Let's trace all 4 ramp portal corners on `showcase_canyon.scmap.ron`:
  1. **Ramp 1 bottom-south `(35, 52)`** (`q11=(35,52)` is `RAMP`, `q00=(34,51)` and `q01=(34,52)` are low-ground `L0`, `q10=(35,51)` is high-ground `L1` cliff):
     - `q01 = (34, 52)` is connected to both `q00=(34,51)` (`L0->L0`) and `q11=(35,52)` (`L0->RAMP`). Thus `q01` is the **unique** `is_ramp_pivot` cell!
     - Its interior direction is `into = (-1, +1)` $\rightarrow$ waypoint is offset to `(35 - clearance, 52 + clearance)` inside `(34, 52)` (`y > 52`, aligned with the ramp entrance row `y = 52`)!
  2. **Ramp 1 top-south `(38, 52)`** (`q01=(37,52)` is `RAMP`, `q10=(38,51)` and `q11=(38,52)` are high-ground `L1`, `q00=(37,51)` is low-ground `L0` cliff):
     - `q11 = (38, 52)` is connected to both `q10=(38,51)` (`L1->L1`) and `q01=(37,52)` (`L1->RAMP`). Thus `q11` is the **unique** `is_ramp_pivot` cell!
     - Its interior direction is `into = (+1, +1)` $\rightarrow$ waypoint is offset to `(38 + clearance, 52 + clearance)` inside `(38, 52)`!
  3. **Ramp 2 bottom-north `(29, 12)`** (`q00=(28,11)` is `RAMP`, `q10=(29,11)` and `q11=(29,12)` are low-ground `L0`, `q01=(28,12)` is high-ground `L1` cliff):
     - `q10 = (29, 11)` is connected to both `q00=(28,11)` (`L0->RAMP`) and `q11=(29,12)` (`L0->L0`). Thus `q10` is the **unique** `is_ramp_pivot` cell!
     - Its interior direction is `into = (+1, -1)` $\rightarrow$ waypoint is offset to `(29 + clearance, 12 - clearance)` inside `(29, 11)` (`y < 12`, strictly inside row `11` instead of on the boundary `y = 12.0`)!
  4. **Ramp 2 top-north `(26, 12)`** (`q10=(26,11)` is `RAMP`, `q00=(25,11)` and `q01=(25,12)` are high-ground `L1`, `q11=(26,12)` is low-ground `L0` cliff):
     - `q00 = (25, 11)` is connected to both `q10=(26,11)` (`L1->RAMP`) and `q01=(25,12)` (`L1->L1`). Thus `q00` is the **unique** `is_ramp_pivot` cell!
     - Its interior direction is `into = (-1, -1)` $\rightarrow$ waypoint is offset to `(26 - clearance, 12 - clearance)` inside `(25, 11)`!
- And if no cell in the quad has `is_ramp_pivot(c)` (e.g., an ordinary non-ramp cliff corner where 3 cells are `L0` and 1 cell is `L1`), summing `into` over all `is_pivot(c)` cells (or keeping the existing `!terrain_step_is_connected` away-sum when `is_ramp_pivot` is empty) preserves 100% of existing non-ramp cliff corner offsets!
- **Impact:** With this 15-line refinement in `offset_corner_waypoint`, `run_funnel` produces valid, passable Euclidean string-pulled paths across **both** Ramp 1 and Ramp 2 on `showcase_canyon.scmap.ron` without ever falling back to `smooth_connected_corridor`! Instead of 8+ staircase cell-center waypoints across the ramp, every bubble follows a 3-waypoint Euclidean string-pulled line.

#### 2.3.4 Mechanism 3D Fix: Stable Route Retention During Local Avoidance (`crates/sim_core/src/movement/locomotion.rs`)

In [`prepare_actor_motion`](../../crates/sim_core/src/movement/locomotion.rs#L314-L371):
- Currently, lines 314–326 check:
  ```rust
  let current_route_valid = !actor.current_funnel_waypoints.is_empty()
      && actor.current_funnel_waypoints.iter().all(|&point| {
          let passable = self.pathfinder.segment_is_passable(&self.map, &req, previous, point);
          previous = point;
          passable
      });
  if !current_route_valid {
      proposal.clear_route = true;
  }
  ```
- When a bubble is nudged laterally by $0.05\text{ m}$ near a corner while following a valid funnel route, `segment_is_passable(actor.position, first_wp)` (which checks footprint clearance along the supercover ray) can momentarily return `false` even though the terrain cells between `actor.position` and `first_wp` are still connected and no dynamic buildings were placed. Wiping `actor.current_funnel_waypoints` and re-running A* every tick causes the unit's `first_wp` to jump back and forth.
- **Fix:**
  - When `self.config.vision_aware_pathfinding` is `true`, keep the full `segment_is_passable` check so newly revealed fog-of-war obstacles trigger immediate re-pathing (`fog_of_war_tests.rs`).
  - When `self.config.vision_aware_pathfinding` is `false`, validate static route segments `(w_i, w_{i+1})` with `segment_is_passable`, and for the active dynamic leg `(actor.position, first_wp)` validate that the destination cell of `first_wp` is passable and that `MovementPolicy::for_domain(actor.movement_domain).permits_segment(&self.map, actor.position, first_wp)` holds (no unwalkable cells or cliff walls between `actor.position` and `first_wp`).
  - Furthermore, when `Advance { next_goal, should_repath: false }` is returned, advance along the existing `actor.current_funnel_waypoints` (popping reached waypoints) without calling `find_funnel_path` on every tick!

---

## 3. Step-by-Step Implementation Plan

### Step 1: Update Default Smog Parameters (`5.0` / `3.4`) and `SwarmStressTest` Volume Bounds (`crates/client`)

1. **[`crates/client/src/smog/mod.rs`](../../crates/client/src/smog/mod.rs):**
   - In `SmogConfig::default()` (lines 94–108), set:
     - `emission_rate: 5.0`
     - `wake_influence: 3.4`
   - In `update_smog_field_system` (lines 675–718), add the explicit `crate::LabTab::SwarmStressTest` match arm:
     ```rust
     let (target_min, target_max) = match current_tab {
         crate::LabTab::VisualLab => (Vec3::new(-6.5, 0.0, -4.5), Vec3::new(6.5, 2.8, 4.5)),
         crate::LabTab::SwarmStressTest => (Vec3::new(4.0, 0.0, 7.0), Vec3::new(28.0, 2.8, 24.0)),
         crate::LabTab::LiveBattle => {
             // Existing LiveBattle camera/unit bounds logic
             ...
         }
     };
     ```
   - In `update_smog_field_system` (line 774), enable `needs_prewarm` for both `LiveBattle` and `SwarmStressTest`:
     ```rust
     let needs_prewarm = matches!(
         current_tab,
         crate::LabTab::LiveBattle | crate::LabTab::SwarmStressTest
     ) && stats.total_density < 0.5
         && query.iter().count() > 0;
     ```
   - In `update_smog_field_system` (lines 810–814), unify `(emission_strength, influence_strength)` to `(1.0, 1.0)` for all bubbles so that `emission_rate = 5.0` and `wake_influence = 3.4` apply uniformly across all tabs.

2. **[`crates/client/src/lab_ui.rs`](../../crates/client/src/lab_ui.rs):**
   - In `LabState::default()` (lines 165–170), set:
     - `swarm_active: true`
     - `smog_emission_rate: 5.0`
     - `smog_wake_influence: 3.4`

3. **[`crates/client/src/scene_controller.rs`](../../crates/client/src/scene_controller.rs):**
   - In `sync_swarm_stress_system` (around line 640–840), add `mut camera_query: Query<&mut crate::camera::RtsCamera>` (or include it in the system parameters without conflicting with existing queries) so that when the interactive 200-bubble swarm is spawned (`controlled_fixture.is_none()`), `cam.target = Vec2::new(16.0, 15.4)`, `cam.look_at_y = 0.5`, and `cam.distance = 22.0` frame the 200 bubbles and their smog wakes directly in the viewport.
   - In the `controlled_fixture.is_none()` spawn loop (lines 800–832), insert `crate::mist::MistMotion { conveyor_velocity: None, enabled: false }` alongside `SwarmBubble` so `MistWakePlugin` does not allocate legacy 2D ribbon slots for the 200 smog-driven bubbles.

### Step 2: Fix Ramp Portal Corner Offsets in `offset_corner_waypoint` (`crates/sim_map/src/pathfinding.rs`)

1. **[`crates/sim_map/src/pathfinding.rs`](../../crates/sim_map/src/pathfinding.rs):**
   - In `offset_corner_waypoint` (lines 1149–1173), when `blocked_count == 0 && vx > 0 && vy > 0 && !map.terrain_step_is_connected(q00, q11)`:
     - Check the four quad cells and their orthogonal quad neighbors:
       ```rust
       let quad = [
           (q00, [q10, q01], -1_i32, -1_i32),
           (q10, [q00, q11], 1_i32, -1_i32),
           (q01, [q00, q11], -1_i32, 1_i32),
           (q11, [q10, q01], 1_i32, 1_i32),
       ];
       let mut ramp_pivot_x = 0_i32;
       let mut ramp_pivot_y = 0_i32;
       let mut has_ramp_pivot = false;
       for &(cell, orth, into_x, into_y) in &quad {
           let connected_to_both = orth
               .iter()
               .all(|&nbr| map.terrain_step_is_connected(cell, nbr));
           let touches_ramp = orth.iter().any(|&nbr| {
               map.cell(nbr.0, nbr.1)
                   .is_ok_and(|c| c.flags.contains(CellFlags::RAMP))
           });
           if connected_to_both && touches_ramp {
               ramp_pivot_x += into_x;
               ramp_pivot_y += into_y;
               has_ramp_pivot = true;
           }
       }
       if has_ramp_pivot {
           dir_x = ramp_pivot_x;
           dir_y = ramp_pivot_y;
       } else {
           // Existing fallback for non-ramp disconnected corners
           for &(cell, orth, away_x, away_y) in &[
               (q00, [q10, q01], 1_i32, 1_i32),
               (q10, [q00, q11], -1_i32, 1_i32),
               (q01, [q00, q11], 1_i32, -1_i32),
               (q11, [q10, q01], -1_i32, -1_i32),
           ] {
               if orth
                   .iter()
                   .any(|&nbr| !map.terrain_step_is_connected(cell, nbr))
               {
                   dir_x += away_x;
                   dir_y += away_y;
               }
           }
       }
       ```
   - Verify that `find_funnel_path` on `showcase_canyon.scmap.ron` across both Ramp 1 and Ramp 2 returns clean 3–4 waypoint Euclidean string-pulled funnel routes without falling back to `smooth_connected_corridor`.

### Step 3: Smooth Local Steering, Co-Moving Separation, and Route Retention (`crates/sim_core`)

1. **[`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs):**
   - In `compute_unified_local_steering` (lines 161–223):
     - Keep all radius and input validations (`unit_radius <= 0`, `obs.radius <= 0`).
     - Keep `lookahead = 0.35`, `combined = unit_radius + obs.radius`, `threshold = combined + lookahead`, `within_distance(unit_pos, obs.pos, threshold)`, and `forward > 0`.
     - For normal in-range coordinates (`let Ok(rel) = checked_vec_sub(obs.pos, unit_pos)`):
       - **Co-moving squadmate filter:** Compute `obs_forward = difference_product(goal_dir, obs.velocity, SimVec2::ZERO, false)?`. If `obs_forward > SimScalar::ZERO && !within_distance(unit_pos, obs.pos, i128::from(combined.raw()))`, `continue` (do not swerve around a non-overlapping neighbor already moving in the same direction).
       - **Lateral corridor clearance check:** Compute `left_normal = SimVec2::new(checked_scalar_neg(goal_dir.y)?, goal_dir.x)` and `lat = checked_dot(rel, left_normal)?`. Let `corridor = checked_scalar_add(combined, SimScalar::from_f32_lossy(0.05))?`. If `lat.abs() >= corridor`, `continue`.
       - **Zero-deadzone side selection & smooth weighted tangent sum:**
         - `away = if lat < SimScalar::ZERO { left_normal } else if lat > SimScalar::ZERO { SimVec2::new(goal_dir.y, checked_scalar_neg(goal_dir.x)?) } else if unit_id.0 < obs.id.0 { left_normal } else { SimVec2::new(goal_dir.y, checked_scalar_neg(goal_dir.x)?) }`.
         - Compute `dist = rel.length()`, `max_dist = checked_scalar_add(combined, lookahead)?`.
         - `radial_pen = checked_scalar_sub(max_dist, dist)?.max(SimScalar::ZERO)`.
         - `radial_t = checked_div(radial_pen, lookahead)?.clamp(SimScalar::from_f32_lossy(0.15), SimScalar::ONE)`.
         - `lateral_pen = checked_scalar_sub(corridor, lat.abs())?.max(SimScalar::ZERO)`.
         - `lateral_t = checked_div(lateral_pen, corridor)?.clamp(SimScalar::from_f32_lossy(0.15), SimScalar::ONE)`.
         - `weight = checked_mul(SimScalar::from_f32_lossy(0.65), checked_mul(radial_t, lateral_t)?)?`.
         - Accumulate `tangent_sum = checked_vec_add(tangent_sum, checked_vec_scale(away, weight)?)?`.
     - For extreme coordinates where `checked_vec_sub(obs.pos, unit_pos)` overflows (`Err`), preserve the existing `difference_product` fallback so `public_steering_handles_extreme_directions_and_wide_obstacle_thresholds` and `legacy_lookahead_and_arrival_raw_thresholds_keep_boundary_branch` pass unchanged.
   - In `checked_dynamic_push` (lines 491–521):
     - Check `let co_moving = a_moving && b_moving && checked_dot(a.vel, b.vel)? > SimScalar::ZERO;`.
     - When `co_moving` is `true`, use smooth radial separation (`correction = checked_vec_scale(separation, half)?`) without the discontinuous `|cos θ| > 0.5` wedge jump.

2. **[`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs):**
   - In `prepare_actor_motion` (lines 314–371):
     - Validate the active route without needlessly wiping it when a unit makes a small local-avoidance offset:
       - If `self.config.vision_aware_pathfinding` is `true`, preserve the per-segment `segment_is_passable` check so newly revealed fog-of-war blockers trigger immediate re-pathing.
       - If `self.config.vision_aware_pathfinding` is `false`, check that `actor.current_funnel_waypoints` is non-empty, that `MovementPolicy::for_domain(actor.movement_domain).permits_segment(&self.map, actor.position, actor.current_funnel_waypoints[0])` holds, and that all subsequent static segments `(w_i, w_{i+1})` satisfy `self.pathfinder.segment_is_passable(&self.map, &req, w_i, w_{i+1})`.
       - When `Advance { next_goal, should_repath: false }` is returned and `current_route_valid` is `true`, advance along the existing `actor.current_funnel_waypoints` (skipping any waypoints already within `arrival_radius` of `actor.position`) instead of re-running A* from scratch on every tick.

---

## 4. Verification & Test Plan

1. **Smog Defaults & 200-Unit Swarm Mist Verification (`crates/client/tests/smog_bouncing_bubbles_tests.rs`):**
   - Add a dedicated integration test `swarm_stress_test_200_bubbles_populates_smog_field_with_default_5_0_and_3_4` that:
     1. Asserts `SmogConfig::default().emission_rate == 5.0` and `SmogConfig::default().wake_influence == 3.4`, and `LabState::default().smog_emission_rate == 5.0`, `LabState::default().smog_wake_influence == 3.4`, `LabState::default().swarm_active == true`.
     2. Initializes a headless Bevy `App` with `LabState { current_tab: LabTab::SwarmStressTest, ..Default::default() }`, `SmogPlugin`, and `sync_swarm_stress_system`.
     3. Runs two `app.update()` ticks and asserts that 200 `SwarmBubble` entities are spawned, `SmogStats.active_source_count == 200`, and `SmogStats.total_density > 100.0` (confirming rich mist across the 200-bubble swarm).
2. **Smooth Bubble Path Verification (`crates/sim_core/tests/collision_and_immobility_tests.rs` & `crates/sim_map/tests/funnel_tests.rs`):**
   - Add a funnel test in `crates/sim_map/tests/funnel_tests.rs` verifying that `find_funnel_path` across both Ramp 1 (`(20.5, 55.5) -> (48.5, 42.5)`) and Ramp 2 (`(43.5, 8.5) -> (15.5, 21.5)`) on `showcase_canyon.scmap.ron` produces a smooth $\le 4$-waypoint Euclidean funnel route (not a 10+ waypoint staircase fallback).
   - Add a path-smoothness behavioral test in `crates/sim_core/tests/collision_and_immobility_tests.rs` (`battlefield_bubble_trajectories_are_smooth_without_micro_jitter`) that runs the 16-unit `showcase_canyon.scmap.ron` battlefield march for 160 ticks, traces the velocity heading changes $\Delta \theta_t = \arccos(\hat{v}_t \cdot \hat{v}_{t-1})$ of marching bubbles prior to combat engagement, and asserts that:
     - Units make zero sign-reversing zig-zag oscillations ($\text{sign}(\hat{v}_t \times \hat{v}_{t-1}) \cdot \text{sign}(\hat{v}_{t-1} \times \hat{v}_{t-2}) < 0$ with $|\Delta\theta| > 10^\circ$) while marching in formation.
     - Average per-tick angular deviation during the march is $< 2.5^\circ$/tick.
3. **Workspace Quality Gates:**
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --all -- --check`


---

## 5. Final Stage 2 Completion Delta (`locomotion.rs` & `collision_and_immobility_tests.rs`)

### 5.1 Restore Clean `locomotion.rs` State from `task-756`

In [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs):

1. **Imports (lines 5–7):** Remove the unused `checked_vec_sub` import:
   ```rust
   use super::math::{checked_vec_add, checked_vec_scale, distance};
   ```
2. **Ground Route Validity Check (lines 317–330):** Keep active multi-waypoint funnel routes stable during local avoidance while validating static legs:
   ```rust
        if policy.layer == MovementLayer::Ground {
            let valid = if let Some(&first_wp) = actor.current_funnel_waypoints.first() {
                let first_leg_ok = policy.permits_segment(&self.map, actor.position, first_wp);
                let static_legs_ok = actor.current_funnel_waypoints.windows(2).all(|w| {
                    self.pathfinder
                        .segment_is_passable(&self.map, &req, w[0], w[1])
                });
                static_legs_ok && (first_leg_ok || actor.current_funnel_waypoints.len() >= 2)
            } else {
                false
            };
            if !valid {
                proposal.clear_route = true;
            }
        }
   ```
3. **Local Steering Call & `prepare_slide` (lines 422–476 and 588–618):** Remove the experimental `steer_target` blending and `steered_pos` override, and restore `Some((x, velocity))` / `Some((y, velocity))` in `prepare_slide`:
   ```rust
        } else {
            let dist_to_target = distance(actor.position, target).unwrap_or(SimScalar::ZERO);
            let is_corridor =
                proposal.route(actor).len() > 1 || dist_to_target > SimScalar::from_num(4);
            let direction = compute_unified_local_steering_ext(
                id,
                actor.position,
                policy.radius,
                next,
                None,
                obstacles,
                is_corridor,
            )
            .map_err(|e| arithmetic(id, e))?;
            prepare_slide(
                &self.map,
                policy,
                actor,
                &mut proposal,
                checked_vec_scale(direction, speed).map_err(|e| arithmetic(id, e))?,
            )
            .map_err(|e| arithmetic(id, e))?;
        }
   ```
   and in `prepare_slide`:
   ```rust
        let x = SimVec2::new(next.x, actor.position.y);
        let y = SimVec2::new(actor.position.x, next.y);
        if policy.permits_segment(map, actor.position, x) {
            Some((x, velocity))
        } else if policy.permits_segment(map, actor.position, y) {
            Some((y, velocity))
        } else {
            None
        }
   ```

### 5.2 Clean Up Debug Output & Finalize `collision_and_immobility_tests.rs`

In [`crates/sim_core/tests/collision_and_immobility_tests.rs`](../../crates/sim_core/tests/collision_and_immobility_tests.rs):

1. Remove the temporary `if tick % 25 == 0 { println!(...); }` block in `test_live_battle_10_unit_squad_ramp_traversal` (lines 653–656).
2. Remove the temporary `prev_wps` and `println!` debug blocks in `test_live_battle_scenario_dual_ramp_traversal` (lines 845, 848–853, 868–874).
3. Remove the temporary `#[test] fn test_debug_path()` function at the end of the file (lines 1090–1127).
4. In `battlefield_bubble_trajectories_are_smooth_without_micro_jitter`:
   - Remove temporary `println!` statements.
   - Use `if d_dist >= 0.02` (filtering out near-stationary wall-contact micro-slides `< 0.02 m/tick` vs. normal march speed `0.13125 m/tick`) when sampling position displacement headings, so that both velocity heading and position displacement verify `0` zig-zag oscillations and `< 3.0 deg/tick` average angular deviation (`0.38 deg/tick` velocity, `0.44 deg/tick` position).
