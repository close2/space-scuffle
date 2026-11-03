# Stage 1 Architectural Blueprint: StarCraft II Authentic Pathfinding, Boid Flocking & Push Priority

**Date:** 2026-10-30  
**Status:** Approved Stage 1 Blueprint  
**Target Crates:** `crates/sim_core`, `crates/sim_map`

---

## 1. Executive Summary & Architectural Comparison with StarCraft II

In Blizzard's StarCraft II engine (as documented by lead pathfinding engineer James Anhalt in *"Crowds in a Polygon Soup: Next-Gen Path Planning in StarCraft II"*), unit movement is governed by three strictly decoupled layers:

1. **Macro Corridor String-Pulling (Funnel Pathfinding with Safe Portal Entry/Exit):**
   - Units follow string-pulled funnel waypoints across corridor portals (including narrow ramp entrances and exits).
   - A unit does **not** abandon an intermediate corner/portal waypoint while still outside that waypoint's portal cell (`0.75 m` early), nor does it ignore a broken line-of-sight (`!permits_segment`) until it slams into a cliff wall.
2. **Micro Flocking / Boid Steering (`velocity` integration):**
   - **Co-Moving Squadmates (`obs.velocity != 0` in the same hemisphere):** Units marching together act as a **Boid flock**:
     - Squadmates ahead in the forward cone (`proj > lat`) act as **column leaders** and produce zero lateral swerve.
     - Squadmates alongside within a soft separation shell (`dist < r_1 + r_2 + 0.08`) apply a gentle, continuous **lateral separation weight** (`<= 0.35`) so parallel lanes form smoothly in `velocity` without $40^\circ$ swerves and without colliding.
   - **Stationary / Opposing Blockers (`obs.velocity == 0` or `dot(goal_dir, obs.velocity) < -0.15`):** Idle allies, `HoldPosition` walls, sieged units, anchored ranged attackers, and opposing units trigger full **tangential bypass steering** (`<= 0.85`), allowing moving units to wrap smoothly around stationary formations and form concave firing arcs.
3. **Post-Motion Contact & Push Priority Resolution (`position` non-penetration):**
   - **Do units in SC2 push non-stationary units?**
     - **No.** In SC2, `PushPriority` lateral yielding (`perp * overlap`) applies **exclusively** when a **moving** friendly unit contacts a **stationary (`Idle` / `velocity == 0`)** friendly unit of equal or lower `PushPriority` that is not holding position or anchored in combat.
     - When **two moving units** overlap (`a_moving && b_moving`), neither unit shoves the other sideways with a perpendicular wedge. Their contact constraint is resolved by **pure symmetric radial separation along the line of centers** (`+0.5 * overlap * separation`, `-0.5 * overlap * separation`).

---

## 2. Root-Cause Diagnosis of Remaining Non-SC2 Behaviors

### 2.1 Moving-vs-Moving Perpendicular Wedge in `checked_dynamic_push` (`crates/sim_core/src/steering.rs`)

In [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L683-L716):
```rust
    let half = overlap * SimScalar::HALF;
    let move_dir = if a_moving {
        checked_normalize(a.vel)?
    } else if b_moving {
        checked_normalize(b.vel)?
    } else {
        SimVec2::ZERO
    };
    if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON
        && checked_dot(separation, move_dir)?.raw().unsigned_abs() > SimScalar::HALF.raw() as u64
    {
        ...
        let wedge_dir_a = checked_normalize(checked_vec_add(
            separation,
            checked_vec_scale(perp, SimScalar::from_f32_lossy(0.25))?,
        )?)?;
        ...
    }
```
- **Why this violates SC2 physics and causes trajectory jitter:**
  1. When two moving friendly units (`a_moving && b_moving`) touch while marching in formation, this branch takes `move_dir` from `a.vel` alone and adds a discontinuous `0.25 * perp` sideways shove whenever `|cos θ| > 0.5`.
  2. Because `resolve_spatial_overlap_pushes` modifies `actor.position` *after* locomotion, shoving two co-moving units sideways at the end of tick $t$ while their `velocity` points toward the waypoint creates a tick-by-tick position sawtooth and pushes flank units laterally outside narrow ramp approach corridors.
- **SC2 Fix:**
  - Remove the `0.25 * perp` moving-vs-moving wedge branch (lines 683–714) completely.
  - Keep the exact SC2 rules in `checked_dynamic_push`:
    1. Hostile units (`a.owner != b.owner`): 50/50 pure radial separation (`raw_overlap * 0.5 * separation`).
    2. Immobile / `HoldPosition` / combat-anchored unit vs. mobile friendly unit (`b_immobile && !a_immobile` or `a_immobile && !b_immobile`): immobile unit has `delta = 0`; moving unit slides radially + tangentially around the immovable blocker.
    3. Moving friendly unit vs. **stationary (`!b_nonzero`)** friendly unit with `a.push_priority >= b.push_priority` (and symmetric `!a_nonzero && b_nonzero && b.push_priority >= a.push_priority`): moving unit has `delta = 0`; stationary unit yields laterally (`perp * overlap`).
    4. All other dynamic pairs (including **all moving-vs-moving pairs `a_nonzero && b_nonzero`**): pure symmetric radial non-penetration along the line of centers:
       ```rust
       let half = overlap * SimScalar::HALF;
       let correction = checked_vec_scale(separation, half)?;
       Ok((correction, checked_vec_neg(correction)?))
       ```

---

### 2.2 Co-Moving Boid Separation vs. Stationary Blocker Bypass (`crates/sim_core/src/steering.rs`)

In [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs#L164-L227) (`calc_steering_weight` and `compute_unified_local_steering_ext`):
- Currently, `calc_steering_weight` uses the same `0.85` ($40.4^\circ$) maximum tangent weight `(w_touch, w_max) = (0.85, 0.85)` for both stationary blockers (`obs.velocity == SimVec2::ZERO`) and co-moving squadmates (`obs.velocity != SimVec2::ZERO`), and ignores `_ = obs_forward`.
- **SC2 Fix:**
  1. In `compute_unified_local_steering_ext` (lines 363–372):
     - When `is_corridor` is `true` and `obs` is co-moving (`obs.velocity != SimVec2::ZERO && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15)`):
       - Column leaders ahead (`proj_raw > lat_raw`) are skipped (`continue`) so trailing units march straight behind their column leader.
       - Side-by-side squadmates (`proj_raw <= lat_raw`) use a soft Boid separation shell `boid_threshold = combined + i128::from(SimScalar::from_f32_lossy(0.08).raw())`. If `!within_distance(unit_pos, obs.pos, boid_threshold)`, skip (`continue`).
  2. In `calc_steering_weight` (lines 164–227):
     - Distinguish co-moving flockmates (`let is_co_moving = is_corridor && blocker.velocity != SimVec2::ZERO && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15);`) from stationary/opposing blockers:
       - For **co-moving flockmates (`is_co_moving == true`)**: use a smooth, gentle Boid separation weight `(w_touch, w_max) = (SimScalar::from_f32_lossy(0.25), SimScalar::from_f32_lossy(0.35))` over `threshold = combined + SimScalar::from_f32_lossy(0.08)`. Side-by-side units glide smoothly into parallel lanes (at most $14^\circ$–$19^\circ$ gentle lane adjustment when overlapping, ramping smoothly to $0^\circ$ at $0.08\text{ m}$ separation).
       - For **stationary or opposing blockers (`is_co_moving == false`)**: keep `(w_touch, w_max) = (SimScalar::from_f32_lossy(0.85), SimScalar::from_f32_lossy(0.85))` over `threshold = combined + SimScalar::from_f32_lossy(0.35)` so units decisively bypass stationary obstacles and fan out into concave firing arcs.

---

### 2.3 SC2 Ramp Portal Waypoint Handoff & Corridor Line-of-Sight Guard (`crates/sim_core/src/movement/locomotion.rs`)

Tracing `EntityId(13)` (a `Large` unit with `policy.radius = 0.75`) approaching Ramp 2 (`x = 26..=28, y = 8..=11`) revealed why both `task-756` and `task-776` had a flaw at the ramp entrance:

1. **Why `task-756` hit the cliff wall at `(25.9895, 12.5067)` at tick 170:**
   - In `task-756`, `locomotion.rs` had:
     `static_legs_ok && (first_leg_ok || actor.current_funnel_waypoints.len() >= 2)`
   - Because `actor.current_funnel_waypoints` had 2+ waypoints (`[(29.481, 11.519), ...]`), `locomotion.rs` **ignored** `first_leg_ok == false` (`!policy.permits_segment(&self.map, actor.position, first_wp)`) when `Unit 13` was at `y > 12.0` (`(23.0, 12.4)`), so `Unit 13` never repathed to add the ramp-entrance waypoint `(25.519, 11.519)` and walked straight into the cliff wall at `x = 26.0, y = 12.50`!
2. **Why `task-776` oscillated at ticks 161..177 when `first_leg_ok` WAS required:**
   - Look at lines 353–370 of [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs#L353-L370):
     ```rust
        while proposal.route(actor).len() >= 2 {
            let first = proposal.route(actor)[0];
            let subsequent = proposal.route(actor)[1];
            let d = distance(actor.position, first).map_err(|e| arithmetic(id, e))?;
            let close = d <= speed.max(policy.radius);
            ...
     ```
   - When `Unit 13` (`policy.radius = 0.75`) repathed from `y > 12.0` and received the ramp-entrance corner waypoint `first = (25.519, 11.519)` (in cell `(25, 11)`), `Unit 13` walked toward `(25.519, 11.519)`.
   - As soon as `Unit 13` reached `(25.05, 12.10)` (`d = 0.74 <= policy.radius = 0.75`), **`Unit 13` was still in cell `(25, 12)` (`y = 12.10 > 12.0`, outside the ramp row `y = 11`)**, yet `close = d <= speed.max(policy.radius)` evaluated to `true` and **prematurely popped `first = (25.519, 11.519)`**!
   - Because `Unit 13` was still at `y = 12.10 > 12.0`, turning East toward `subsequent = (29.481, 11.519)` immediately grazed the cliff corner `(26, 12)` (`permits_segment == false`), causing `locomotion.rs` to wipe the route, re-insert `first = (25.519, 11.519)`, pop it again on the next tick, and repeat in a 2-tick loop!
3. **The Exact SC2 Fix in `locomotion.rs`:**
   - **Rule A — Require `first_leg_ok && static_legs_ok` for Route Validity (lines 317–330):**
     ```rust
        if policy.layer == MovementLayer::Ground {
            let valid = if let Some(&first_wp) = actor.current_funnel_waypoints.first() {
                policy.permits_segment(&self.map, actor.position, first_wp)
                    && actor.current_funnel_waypoints.windows(2).all(|w| {
                        self.pathfinder
                            .segment_is_passable(&self.map, &req, w[0], w[1])
                    })
            } else {
                false
            };
            if !valid {
                proposal.clear_route = true;
            }
        }
     ```
   - **Rule B — Tight Corner-Waypoint Handoff inside `while proposal.route(actor).len() >= 2` (lines 353–370):**
     - An intermediate corner waypoint `first` was offset by `offset_corner_waypoint` into a specific passable pivot cell (e.g. `(25.519, 11.519)` in cell `(25, 11)`). Popping `first` while `d = 0.75` (`policy.radius`) cuts the corner by `0.75 m` while the unit is still in the adjacent blocked-row cell `(25, 12)`!
     - Instead, only hand off from `first` to `subsequent` when `actor.position` is within `handoff_dist = speed.max(SimScalar::from_f32_lossy(0.28))` of `first` AND in the same grid cell as `first` (or within a single step `d <= speed`), AND both `policy.permits_segment(&self.map, actor.position, subsequent)` and `self.pathfinder.segment_is_passable(&self.map, &req, actor.position, subsequent)` hold:
     ```rust
        while proposal.route(actor).len() >= 2 {
            let first = proposal.route(actor)[0];
            let subsequent = proposal.route(actor)[1];
            let d = distance(actor.position, first).map_err(|e| arithmetic(id, e))?;
            let same_cell = self.map.world_to_cell(actor.position).ok()
                == self.map.world_to_cell(first).ok();
            let handoff_radius = speed.max(SimScalar::from_f32_lossy(0.28));
            let close = d <= speed || (d <= handoff_radius && same_cell);
            let subsequent_passable = match policy.layer {
                MovementLayer::Air => policy.permits_segment(&self.map, actor.position, subsequent),
                MovementLayer::Ground => {
                    policy.permits_segment(&self.map, actor.position, subsequent)
                        && self
                            .pathfinder
                            .segment_is_passable(&self.map, &req, actor.position, subsequent)
                }
            };
            if close && subsequent_passable {
                proposal.remove_prefix += 1;
            } else {
                break;
            }
        }
     ```
     - Why this is mathematically rock-solid:
       1. When `first = (25.519, 11.519)` (in cell `(25, 11)`), `close` cannot become `true` until `actor.position` has entered cell `(25, 11)` (`y < 12.0`) AND is within `0.28 m` of `(25.519, 11.519)` (so `y <= 11.519 + 0.28 = 11.799 < 12.0`, at least `0.20 m` inside the ramp corridor row `11`).
       2. Moreover, `subsequent_passable` checks **both** `policy.permits_segment(&self.map, actor.position, subsequent)` (the exact predicate checked by line 319 on the next tick!) AND `segment_is_passable`. Thus, popping `first` can **never** transition to a `subsequent` waypoint that fails `policy.permits_segment` on the next tick!
   - **Rule C — Keep `ground_segment_is_connected` Steering Guard & Clean `prepare_slide` (lines 422–476 & 588–618):**
     - Remove the `steer_target` blending block (lines 425–444), passing `next` directly to `compute_unified_local_steering_ext`.
     - Keep the `ground_segment_is_connected` steering guard (lines 456–467): if `actor.position -> next` is connected across the terrain, local obstacle avoidance is not allowed to steer to a `steered_pos` that disconnects `steered_pos -> next` across a cliff edge.
     - In `prepare_slide` (lines 599–606), restore `Some((x, velocity))` and `Some((y, velocity))` so a 1-tick wall slide does not snap `proposal.velocity` and `proposal.heading` by $90^\circ$.

---

## 3. Step-by-Step Implementation Plan for Stage 2

### Step 1: SC2 Push Priority & Boid Steering in [`crates/sim_core/src/steering.rs`](../../crates/sim_core/src/steering.rs)

1. **In `calc_steering_weight` (lines 164–227):**
   - Compute `obs_fwd_scalar = SimScalar::from_raw((obs_forward >> 32) as i64);`.
   - Identify co-moving flockmates:
     ```rust
     let is_co_moving = is_corridor
         && blocker.velocity != SimVec2::ZERO
         && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15);
     ```
   - Use soft Boid separation parameters for co-moving flockmates vs. full bypass parameters for stationary/opposing obstacles:
     ```rust
     let lookahead_margin = if is_co_moving {
         SimScalar::from_f32_lossy(0.08)
     } else {
         SimScalar::from_f32_lossy(0.35)
     };
     let threshold = combined + lookahead_margin;
     if dist >= threshold {
         return SimScalar::ZERO;
     }
     let (w_touch, w_max) = if is_co_moving {
         (SimScalar::from_f32_lossy(0.25), SimScalar::from_f32_lossy(0.35))
     } else {
         (SimScalar::from_f32_lossy(0.85), SimScalar::from_f32_lossy(0.85))
     };
     ```
2. **In `compute_unified_local_steering_ext` (lines 363–372):**
   - When `is_corridor && obs.velocity != SimVec2::ZERO && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15)`:
     ```rust
     let is_column_leader = proj_raw > lat_raw;
     let boid_threshold = combined + i128::from(SimScalar::from_f32_lossy(0.08).raw());
     let is_outside_boid_shell = !within_distance(unit_pos, obs.pos, boid_threshold);
     if is_column_leader || is_outside_boid_shell {
         continue;
     }
     ```
3. **In `checked_dynamic_push` (lines 682–717):**
   - Remove the moving-vs-moving perpendicular `wedge` block (lines 683–714), replacing lines 682–716 with pure symmetric radial non-penetration:
     ```rust
     let half = overlap * SimScalar::HALF;
     let correction = checked_vec_scale(separation, half)?;
     Ok((correction, checked_vec_neg(correction)?))
     ```

### Step 2: SC2 Waypoint Handoff & Corridor Guard in [`crates/sim_core/src/movement/locomotion.rs`](../../crates/sim_core/src/movement/locomotion.rs)

1. **Imports (lines 5–7):**
   - Remove unused `checked_vec_sub`:
     ```rust
     use super::math::{checked_vec_add, checked_vec_scale, distance};
     ```
2. **Waypoint Handoff Loop (lines 353–370):**
   - Require `d <= speed || (d <= handoff_radius && same_cell)` and `policy.permits_segment(&self.map, actor.position, subsequent)` before popping `first`:
     ```rust
        while proposal.route(actor).len() >= 2 {
            let first = proposal.route(actor)[0];
            let subsequent = proposal.route(actor)[1];
            let d = distance(actor.position, first).map_err(|e| arithmetic(id, e))?;
            let same_cell = self.map.world_to_cell(actor.position).ok()
                == self.map.world_to_cell(first).ok();
            let handoff_radius = speed.max(SimScalar::from_f32_lossy(0.28));
            let close = d <= speed || (d <= handoff_radius && same_cell);
            let subsequent_passable = match policy.layer {
                MovementLayer::Air => policy.permits_segment(&self.map, actor.position, subsequent),
                MovementLayer::Ground => {
                    policy.permits_segment(&self.map, actor.position, subsequent)
                        && self
                            .pathfinder
                            .segment_is_passable(&self.map, &req, actor.position, subsequent)
                }
            };
            if close && subsequent_passable {
                proposal.remove_prefix += 1;
            } else {
                break;
            }
        }
     ```
3. **Local Steering & `prepare_slide` (lines 422–476 & 588–618):**
   - Remove `steer_target` blending (lines 425–444), passing `next` directly to `compute_unified_local_steering_ext`.
   - Keep the `ground_segment_is_connected` steering guard (lines 456–467).
   - Restore `Some((x, velocity))` and `Some((y, velocity))` in `prepare_slide` (lines 599–606).

### Step 3: Unit & Integration Tests in [`crates/sim_core/tests/collision_and_immobility_tests.rs`](../../crates/sim_core/tests/collision_and_immobility_tests.rs)

1. **Add Unit Test `test_sc2_moving_units_never_push_each_other_laterally`:**
   - Construct two moving friendly `PushEntityParams` (`a.vel = (1, 0)`, `b.vel = (1, 0)`, overlapping along X at `(5.0, 10.0)` and `(5.5, 10.0)`).
   - Call `compute_push_displacement(a, b, distance, combined_radius)` and assert:
     - `delta_a.y == SimScalar::ZERO` and `delta_b.y == SimScalar::ZERO` (zero perpendicular/lateral push between moving units).
     - `delta_a.x < SimScalar::ZERO` and `delta_b.x == -delta_a.x` (pure symmetric radial separation along the line of centers).
2. **Clean Up Temporary Debug Output & Verify `battlefield_bubble_trajectories_are_smooth_without_micro_jitter`:**
   - Remove temporary `println!` debug blocks in `collision_and_immobility_tests.rs`.
   - Assert `zig_zags == 0` (velocity) and `pos_zig_zags == 0` (position displacement) with `< 1.0 deg/tick` average angular deviation across the 180-tick battlefield march.

---

## 4. Quality Gate Verification

Stage 2 must run and pass:
1. `cargo test --workspace`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo fmt --all -- --check`
