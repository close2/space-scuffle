# Plan 17: Realistic SC2 Battle Pacing, Melee Timing Pauses, and Colorful Glowing Mist

## 1. Executive Summary & Forensic Root Cause Analysis

### 1.1 Problem Statement & User Observations
In recent playtests of the live combat simulation (`--battle`), the user reported two critical issues:
1. *"I have started the attack in the attack simulation again. If the values are valid units, then the battle is far too soon over. Do close-combat weapons have their pause? It seems as if the enemy units die as soon as a close-range unit reaches the unit."*
2. *"Also the mist is still not looking good. Take a look at the images in tmp. We want something like this. A colorful glowing mist."* (referencing [`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg) and [`damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg`](file:///home/cl/projects/space-scuffle/tmp/damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg)).

### 1.2 Forensic Investigation of the 4 Root Causes

Our deep-dive inspection of `crates/client/src/scene_controller.rs`, `crates/sim_core/src/combat.rs`, `crates/sim_core/src/world.rs`, `assets/catalog/units.ron`, and `assets/shaders/mist_wake.wgsl` uncovered four distinct systemic bugs working in tandem to produce the instant wipe and the poor mist aesthetics:

```
+--------------------------------------------------------------------------------------------------------+
|                                  THE INSTANT BATTLE WIPE COINCIDENCE                                   |
|                                                                                                        |
|  Frame 0: Red Zealots (x=45) move West. Blue Marines (x=15) move East.                                |
|  Frame 60: Contact at x ≈ 41. Blue Marines reach range 7.0 of Red Siege Tanks (x=48).                 |
|                                                                                                        |
|  [BUG 1: Unthrottled Stepping]    ---> Sim runs at 144 Hz (5.76x faster than real-time 25 Hz).         |
|  [BUG 2: Errant Tank Splash]      ---> 2 Tanks fire 40+30 AoE splash every 18 ticks (80 AoE dmg!).     |
|                                        Obliterates 45-HP Marine clump on tick 1 of contact!            |
|  [BUG 3: Cooldown Bypass]         ---> Zealots entering range immediately enter Windup, bypassing      |
|                                        weapon_cooldown_remaining on target switches.                   |
|  [BUG 4: Uncalibrated Catalog]    ---> Tank HP=160 vs 175; Stalker HP=100/80 vs 80/80; Stalker cd=26   |
|                                        vs 34 ticks.                                                    |
+--------------------------------------------------------------------------------------------------------+
```

#### Bug 1: Unthrottled Per-Render-Frame Simulation Stepping in `sync_live_battle_system`
- **Location:** [`crates/client/src/scene_controller.rs:494`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs#L494)
- **Mechanism:**
  `sync_live_battle_system` is scheduled in Bevy's standard `Update` schedule. It did not take `Res<Time>` as a dependency, and contained the loop:
  ```rust
  for _ in 0..speed {
      let mut snap = world.step(&bundle);
      ...
  }
  ```
  On a 144 Hz monitor, this executed 144 simulation steps per real second at `speed = 1`. Because `sim_core` is calibrated to $25\text{ Hz}$ ($0.04\text{s}$ per tick), the simulation was running **5.76 times faster than real time**! At `speed = 2` or `4`, it ran at $288$ or $576\text{ ticks/second}$ ($11.5\times$ to $23\times$ fast-forward). A real-time 6-second battle was over in barely 1 second!
- **Architectural Remedy:**
  Decouple the simulation tick rate from the display render frame rate. Add `pub battle_tick_accumulator_secs: f32` to `LabState`. In `sync_live_battle_system`, consume `time.delta_secs() * (speed as f32)` in discrete $0.04\text{s}$ increments, capped at a maximum of 5 ticks per frame to prevent a spiral of death during frame drops.

#### Bug 2: `railgun_crawler` (Unsieged Tank Mode) Erroneously Given Siege Mode $40+30$ AoE Splash Every 18 Ticks
- **Location:** [`assets/catalog/units.ron:334-360`](file:///home/cl/projects/space-scuffle/assets/catalog/units.ron#L334-L360)
- **Mechanism:**
  In `new_default_battle_sandbox()` ([`crates/sim_core/src/world.rs:321-346`](file:///home/cl/projects/space-scuffle/crates/sim_core/src/world.rs#L321-L346)), Red army has 8 `psi_grey` (Zealots) at $x = 45$ and 2 `railgun_crawler` (Siege Tanks) at $x = 48$. Blue army has 10 `pulse_bot` (Marines, 45 HP) at $x = 15$ and 2 `rhino_beetle` (Marauders) at $x = 12$.
  In `units.ron`, `railgun_crawler`'s un-sieged primary weapon was configured with:
  ```ron
  base_damage: 40.0,
  bonus_armored: 30.0,
  period_ticks: 18,
  splash: Some((
      inner_radius: 0.4687,
      middle_radius: 0.7812,
      outer_radius: 1.25,
      friendly_fire: true,
  )),
  ```
  In StarCraft II, **Tank Mode deals 15 damage (+10 vs Armored = 25)**, fires every $1.04\text{s}$ ($26\text{ ticks}$), and has **zero splash (`splash: None`)**! Only **Siege Mode deals 40 (+30 vs Armored = 70)** with AoE splash every $2.14\text{s}$ ($54\text{ ticks}$).
  Because `railgun_crawler`'s unsieged weapon had 40+30 splash every 18 ticks (0.72s) with `Hitscan`, the **exact tick** that the Red Zealots met the Blue Marines at $x \approx 41$ (which is $48 - 41 = 7.0$ range from the tanks), the 2 Red Tanks fired **two invisible 40-damage splash blasts (80 AoE damage!)**, instantaneously annihilating the entire clump of 10 Blue Marines (45 HP) in a single tick! The user perceived this as *"the enemy units die as soon as a close-range unit reaches the unit."*
- **Architectural Remedy:**
  Restore `railgun_crawler`'s un-sieged weapon to authentic SC2 Tank Mode stats: `base_damage: 15.0`, `bonus_armored: 10.0`, `period_ticks: 26`, `damage_point_ticks: 4`, `backswing_ticks: 4`, and `splash: None`. Move the $40.0 + 30.0$ AoE splash weapon strictly into its `SiegeModeToggle` ability's `sieged_weapon` with `period_ticks: 54`.

#### Bug 3: `step_entity_weapon` Missing Cooldown Guard in `Ready` & `RotatingToTarget` + Target-Switch Cooldown Reset
- **Location:** [`crates/sim_core/src/combat.rs:334-433`](file:///home/cl/projects/space-scuffle/crates/sim_core/src/combat.rs#L334-L433)
- **Mechanism:**
  In `step_entity_weapon`:
  ```rust
  match entity.weapon_phase {
      WeaponPhase::Ready => {
          if let Some((target_pos, target_radius)) = target_opt {
              if is_within_firing_arc(...) {
                  entity.weapon_phase = WeaponPhase::Windup {
                      target_id,
                      remaining_ticks: weapon.damage_point_ticks,
                  };
              }
          }
      }
      WeaponPhase::RotatingToTarget { target_id } => {
          if is_within_firing_arc(...) {
              entity.weapon_phase = WeaponPhase::Windup {
                  target_id,
                  remaining_ticks: weapon.damage_point_ticks,
              };
          }
      }
  }
  ```
  Neither `WeaponPhase::Ready` nor `WeaponPhase::RotatingToTarget` checked `if entity.weapon_cooldown_remaining == 0`!
  If an entity's `weapon_phase` was reset to `Ready` while `weapon_cooldown_remaining > 0` (for example, when a target died and a new one was acquired in `acquire_targets`, or when an entity rotated slightly), it skipped the remaining weapon cooldown and immediately began `Windup`, firing within `damage_point_ticks`!
  Furthermore, melee weapons had artificially fast `damage_point_ticks: 1` or `2` (0.04s–0.08s) rather than realistic SC2 frontswing values (`damage_point_ticks: 4` = 0.16s).
- **Architectural Remedy:**
  In `step_entity_weapon`, allow entities in `Ready` or `RotatingToTarget` to track and rotate towards targets while cooling down, but strictly forbid transitioning to `WeaponPhase::Windup` unless `entity.weapon_cooldown_remaining == 0`.
  In `units.ron`, assign realistic front-swing windups (`damage_point_ticks: 4`) and backswings (`backswing_ticks: 6`) to melee weapons (`psi_grey`, `welder_drone`, `razor_mantis`), guaranteeing a distinct, readable windup animation and rhythm between strikes.

#### Bug 4: Visual Mist Aesthetics & Shader Deficiencies
- **Location:** [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl) and [`crates/client/src/vfx.rs:59-71, 265-371`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs#L59-L71)
- **Mechanism:**
  The current mist implementation:
  1. Uses `AlphaMode::Blend` with `alpha <= 0.18`. Standard alpha blending with low alpha against dark floor textures looks like faint, muddy, blue-grey smudges rather than a luminous energy wake.
  2. Uses a monochrome cyan color ramp (`0.55, 0.95, 1.0` to `0.20, 0.75, 0.95`). There is zero violet, magenta, pink, iridescent rainbow, gold, micro-bubbles, or speed streaks.
  3. Uses unrotated UV coordinates on tiny circular quads (`radius * 0.85`), failing to sweep back along the sphere's movement vector `trail_dir`.
- **Architectural Remedy:**
  Switch `MistWakeMaterial` to `AlphaMode::Add` (Additive Blending). In additive blending (`src_color * src_alpha + dst_color`), overlapping wisps bloom into vibrant, luminous highlights and can **never** darken the arena floor or look muddy.
  Re-architect `mist_wake.wgsl` to sample a rich, multi-color iridescent palette (violet-magenta core, electric cyan-blue, pearlescent pink-white, and fiery golden-amber / orange-coral outer rim ribbons), aligned along the movement vector with longitudinal slipstream streaks, procedural iridescent micro-bubbles, and glittering star-sparks matching [`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg).

---

## 2. Authoritative StarCraft II (LotV) Combat Benchmarks & Mathematical Timings

### 2.1 The StarCraft II Time Base: "Faster" Speed Translation
StarCraft II competitive multiplayer is played at **Faster** speed ($1.4\times$ Normal speed).
In *Space Scuffle*, our simulation tick rate is defined in [`sim_types`](file:///home/cl/projects/space-scuffle/crates/sim_types/src/lib.rs) as:
$$\text{TICKS\_PER\_SECOND} = 25\quad (\Delta t = 0.04\text{s per tick})$$
All weapon periods, damage points, cooldowns, and speeds translate directly between SC2 Faster seconds and simulation ticks:
$$\text{ticks} = \text{round}(\text{seconds} \times 25.0)$$

### 2.2 Core Unit Weapon Profiles (SC2 LotV Patch 5.0.14)

| Unit | Slug | HP / Shields | Armor (Hull/Shield) | Weapon Name | Visual Kind | Base Dmg (+Bonus) | Period (Ticks / Sec) | Damage Point (Ticks / Sec) | Range |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Marine** | `pulse_bot` | 45 / 0 | 0 / 0 | Pulse Carbine | Hitscan | 6 (+0) | 15 ticks (0.60s) | 2 ticks (0.08s) | 5.0 |
| **Marauder** | `rhino_beetle` | 125 / 0 | 1 / 0 | Concussive Acid | Projectile (0.64) | 10 (+10 vs Armored) | 27 ticks (1.08s) | 2 ticks (0.08s) | 6.0 |
| **Zealot** | `psi_grey` | 100 / 50 | 1 / 0 | Psi-Blade Slash | MeleeSlash | 8x2 = 16 (+0) | 22 ticks (0.88s) | 4 ticks (0.16s) | 0.3 |
| **Siege Tank (Tank Mode)** | `railgun_crawler` | 175 / 0 | 1 / 0 | Twin Heavy Railgun | Hitscan | 15 (+10 vs Armored) | 26 ticks (1.04s) | 4 ticks (0.16s) | 7.0 |
| **Siege Tank (Siege Mode)** | `railgun_crawler` | 175 / 0 | 1 / 0 | Siege Mode Blast | Hitscan | 40 (+30 vs Armored) | 54 ticks (2.16s) | 4 ticks (0.16s) | 13.0 (min 2.0, AoE) |
| **Stalker** | `warp_strider` | 80 / 80 | 1 / 0 | Particle Disruptor | Projectile (0.75) | 13 (+5 vs Armored) | 34 ticks (1.36s) | 3 ticks (0.12s) | 6.0 |
| **Zergling** | `razor_mantis` | 35 / 0 | 0 / 0 | Scythe Slash | MeleeSlash | 5 (+0) | 12 ticks (0.48s) | 2 ticks (0.08s) | 0.2 |

### 2.3 Benchmark Duels & Small-Squad Mathematical Invariants

#### Benchmark 1: 1 Zealot (`psi_grey`) vs. 1 Marine (`pulse_bot`) at Point-Blank Range
- **Marine HP:** 45. **Marine Armor:** 0.
- **Zealot Attack:** $8 \times 2 = 16$ damage per attack cycle.
- **Damage Sequence upon reaching melee contact:**
  - $t = 0$: Contact made. Weapon in `Ready`, enters `Windup` (`damage_point_ticks = 4`).
  - $t = 4\text{ ticks}$ ($0.16\text{s}$): **Swing 1 lands!** Marine takes 16 dmg $\rightarrow$ **HP drops from 45 to 29**. Weapon enters `Backswing` (6 ticks) then `Cooldown` (12 ticks; total period $4 + 6 + 12 = 22\text{ ticks}$).
  - $t = 26\text{ ticks}$ ($1.04\text{s}$): **Swing 2 lands!** Marine takes 16 dmg $\rightarrow$ **HP drops from 29 to 13**.
  - $t = 48\text{ ticks}$ ($1.92\text{s}$): **Swing 3 lands!** Marine takes 16 dmg $\rightarrow$ **HP drops from 13 to 0 (Marine Dies)**.
- **Invariants:**
  1. The Marine **never dies on the first tick or the first swing**.
  2. The Marine takes **exactly 3 distinct swings**.
  3. Total time-to-kill after melee contact is **at least 48 ticks ($\approx 1.92\text{ seconds}$)**.

#### Benchmark 2: Equal-Cost 1 Zealot (100 Minerals) vs. 2 Marines (100 Minerals) at Distance 5.0
- **Opening (ticks 0..30):** Zealot runs at speed $0.126\text{ u/tick}$. Closes 5.0 distance in $\approx 35\text{ ticks}$.
- **Incoming Fire:** 2 Marines fire at Zealot. Marine damage vs. Zealot shields (0 armor) = 6 dmg/shot. Zealot takes $\approx 24\text{ dmg}$ to shields while running.
- **Melee Engagement:**
  - Zealot reaches Marine 1, kills Marine 1 in 48 ticks (3 swings). Zealot shields break, hull takes $\approx 10\text{ dmg}$ (Marine hull damage vs 1 armor = $6 - 1 = 5\text{ dmg/shot}$).
  - Zealot steps to Marine 2, kills Marine 2 in 48 ticks (3 swings).
- **Result:** Zealot wins over $100..140\text{ ticks}$ ($4.0\text{s}..5.6\text{s}$) with **$65..85\text{ HP}$ remaining**.
- **Scaling:** Against 3 Marines (150 Minerals), 3 Marines deal 18 DPS before armor; fight is razor-close ($125..150\text{ ticks}$). Against 4 Marines (200 Minerals), Marines win with 2 survivors.

#### Benchmark 3: 1 Zealot (`psi_grey`) vs. 1 Marauder (`rhino_beetle`, Un-kited A-Move)
- **Marauder:** 125 HP, 1 Armor. Attacks Zealot for $10\text{ dmg}$ (Zealot is Light/Psionic, NOT Armored; Marauder gets no bonus) every 27 ticks.
- **Zealot:** 100 HP, 50 Shields, 1 Armor. Attacks Marauder for $(8 - 1) \times 2 = 14\text{ dmg}$ every 22 ticks.
- **Time-to-Kill:** Marauder requires $\lceil 125 / 14 \rceil = 9\text{ swings}$.
  $$9 \text{ swings} \times 22\text{ ticks} = 198\text{ ticks}\quad (\approx 7.92\text{s})$$
- **Damage to Zealot:** Over 198 ticks, Marauder attacks $\lfloor 198 / 27 \rfloor = 7\text{ times} \times 10 = 70\text{ dmg}$.
- **Result:** Zealot absorbs 50 shield dmg + 20 hull dmg. **Zealot wins at $\approx 198\text{ ticks}$ with $\approx 80\text{ HP}$ remaining.**

#### Benchmark 4: 1 Marauder (`rhino_beetle`) vs. 1 Stalker (`warp_strider`, Armored Duel)
- **Marauder:** 125 HP, 1 Armor, Armored. Deals $10 + 10 = 20\text{ dmg}$ vs. Armored every 27 ticks.
- **Stalker:** 80 HP, 80 Shields, 1 Armor, Armored. Deals $13 + 5 = 18\text{ dmg}$ vs. Armored every 34 ticks.
- **Marauder vs. Stalker:**
  - Stalker shields (0 armor): 4 shots of $20\text{ dmg} = 80\text{ dmg}$ (depletes shields at tick $4 \times 27 = 108$).
  - Stalker hull (1 armor): $20 - 1 = 19\text{ dmg/shot}$. Requires $\lceil 80 / 19 \rceil = 5\text{ shots}$.
  - Total Marauder shots to kill Stalker: $4 + 5 = 9\text{ shots}$. Time: $9 \times 27\text{ ticks} = 243\text{ ticks}$ ($\approx 9.72\text{s}$).
- **Stalker vs. Marauder:**
  - Marauder hull (1 armor): $18 - 1 = 17\text{ dmg/shot}$. Requires $\lceil 125 / 17 \rceil = 8\text{ shots}$.
  - Time for Stalker to fire 8 shots: $8 \times 34\text{ ticks} = 272\text{ ticks}$ ($\approx 10.88\text{s}$).
- **Result:** Marauder fires 9th shot at tick 243, killing Stalker before Stalker can fire its 8th shot. **Marauder wins with $\approx 20..30\text{ HP}$ remaining.**

#### Benchmark 5: Zergling Swarm (`razor_mantis`) Scaling vs. 1 Zealot
- **1 Zergling:** 35 HP, 0 Armor. Takes 16 dmg from Zealot: $35 \rightarrow 19 \rightarrow 3 \rightarrow 0$ (3 swings, 48 ticks).
- **4 Zerglings (100 Minerals):** Zealot kills 4 Zerglings sequentially; Zealot wins with $\approx 40..60\text{ HP}$.
- **6 Zerglings (150 Minerals):** Surround Zealot; Zerglings win with 2–3 survivors.

#### Benchmark 6: Live Battle Sandbox Engagement Pacing
- In `new_default_battle_sandbox()`: Blue (10 Marines + 2 Marauders) vs. Red (8 Zealots + 2 Tanks in Tank Mode).
- Tanks do **not** splash-kill the army on frame 1 of contact.
- Combat engagement after first weapon firing lasts **at least 150+ ticks ($6.0+\text{ seconds}$)** of sustained tactical fighting.

---

## 3. Core Simulation & Timestep Architecture

### 3.1 Fixed-Timestep Accumulator in `crates/client/src/scene_controller.rs`

```rust
// In crates/client/src/lab_ui.rs (LabState struct):
pub struct LabState {
    ...
    /// Accumulator for fixed 25 Hz simulation ticks in the Live Battle sandbox.
    pub battle_tick_accumulator_secs: f32,
}

// In crates/client/src/scene_controller.rs:
pub fn sync_live_battle_system(
    mut commands: Commands<'_, '_>,
    mut lab_state: ResMut<'_, LabState>,
    time: Res<'_, Time>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
    mut render_entities: ResMut<'_, RenderEntityMap>,
    ...
) {
    ...
    let paused = lab_state.battle_paused;
    let speed = lab_state.simulation_speed;

    let Some(ref mut world) = lab_state.sim_world else {
        return;
    };

    let mut latest_snapshot = None;
    let mut accumulated_combat_events = Vec::new();

    if paused || speed == 0 {
        latest_snapshot = Some(world.build_render_snapshot());
    } else {
        // PERFORMANCE RATIONALE:
        // Why: Running world.step() directly inside Bevy's per-frame Update loop ties
        // simulation speed to monitor refresh rate (60 Hz to 240+ Hz).
        // What: We accumulate delta_secs * speed and step the 25 Hz simulation in fixed
        // 0.04s increments, capping at 5 steps per render frame to prevent spiral of death.
        // Trade-off: Perfectly deterministic 25 Hz simulation regardless of display FPS.
        const SIM_TICK_SECS: f32 = 0.04;
        const MAX_STEPS_PER_FRAME: usize = 5;

        lab_state.battle_tick_accumulator_secs += time.delta_secs() * (speed as f32);

        // Clamp accumulator to avoid spiral of death after system lag spikes or window drags
        if lab_state.battle_tick_accumulator_secs > 0.25 {
            lab_state.battle_tick_accumulator_secs = 0.25;
        }

        let mut steps = 0;
        while lab_state.battle_tick_accumulator_secs >= SIM_TICK_SECS && steps < MAX_STEPS_PER_FRAME {
            lab_state.battle_tick_accumulator_secs -= SIM_TICK_SECS;
            let bundle = TickBundle {
                tick: Tick(world.current_tick),
                commands: Vec::new(),
                state_checksum: None,
            };
            let mut snap = world.step(&bundle);
            accumulated_combat_events.append(&mut snap.combat_events);
            latest_snapshot = Some(snap);
            steps += 1;
        }
    }
```

### 3.2 Weapon State Machine Cooldown Hardening in `crates/sim_core/src/combat.rs`

In `step_entity_weapon`:
```rust
pub fn step_entity_weapon(
    entity: &mut Entity,
    weapon: &WeaponSpec,
    target_opt: Option<(SimVec2, SimScalar)>,
) -> bool {
    // Decrement weapon cooldown if running
    entity.weapon_cooldown_remaining = entity.weapon_cooldown_remaining.saturating_sub(1);

    match entity.weapon_phase {
        WeaponPhase::Ready => {
            if let Some((target_pos, target_radius)) = target_opt {
                let dist = entity.position.distance(target_pos);
                let effective_max = weapon.max_range + target_radius;
                let effective_min = if weapon.min_range > SimScalar::ZERO {
                    weapon.min_range + target_radius
                } else {
                    SimScalar::ZERO
                };
                if dist > effective_max || dist < effective_min {
                    return false;
                }

                let current_aim = if weapon.has_turret {
                    entity.turret_angle
                } else {
                    entity.heading
                };

                let delta = target_pos - entity.position;
                let target_angle = SimAngle::atan2(delta.y, delta.x);

                if is_within_firing_arc(current_aim, entity.position, target_pos, weapon.firing_arc) {
                    // CRITICAL COOLDOWN GUARD:
                    // An entity may align with a target while its weapon is on cooldown,
                    // but MUST NOT start Windup until cooldown has fully elapsed!
                    if entity.weapon_cooldown_remaining == 0 {
                        let target_id = match entity.order_queue.front() {
                            Some(UnitOrder::AttackTarget { target }) => *target,
                            _ => EntityId(0),
                        };
                        entity.weapon_phase = WeaponPhase::Windup {
                            target_id,
                            remaining_ticks: weapon.damage_point_ticks,
                        };
                    }
                } else {
                    // Rotate towards target
                    let target_id = match entity.order_queue.front() {
                        Some(UnitOrder::AttackTarget { target }) => *target,
                        _ => EntityId(0),
                    };
                    entity.weapon_phase = WeaponPhase::RotatingToTarget { target_id };
                    if weapon.has_turret {
                        entity.turret_angle = entity.turret_angle.rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                    } else {
                        entity.heading = entity.heading.rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                    }
                }
            }
            false
        }
        WeaponPhase::RotatingToTarget { target_id } => {
            if let Some((target_pos, target_radius)) = target_opt {
                let dist = entity.position.distance(target_pos);
                let effective_max = weapon.max_range + target_radius;
                let effective_min = if weapon.min_range > SimScalar::ZERO {
                    weapon.min_range + target_radius
                } else {
                    SimScalar::ZERO
                };
                if dist > effective_max || dist < effective_min {
                    entity.weapon_phase = WeaponPhase::Ready;
                    return false;
                }

                let delta = target_pos - entity.position;
                let target_angle = SimAngle::atan2(delta.y, delta.x);

                if weapon.has_turret {
                    entity.turret_angle = entity.turret_angle.rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                } else {
                    entity.heading = entity.heading.rotate_towards(target_angle, weapon.turret_turn_rate.raw());
                }

                let current_aim = if weapon.has_turret {
                    entity.turret_angle
                } else {
                    entity.heading
                };

                if is_within_firing_arc(current_aim, entity.position, target_pos, weapon.firing_arc) {
                    if entity.weapon_cooldown_remaining == 0 {
                        entity.weapon_phase = WeaponPhase::Windup {
                            target_id,
                            remaining_ticks: weapon.damage_point_ticks,
                        };
                    } else {
                        // Aligned, but still cooling down: transition back to Ready to await cooldown expiry
                        entity.weapon_phase = WeaponPhase::Ready;
                    }
                }
            } else {
                entity.weapon_phase = WeaponPhase::Ready;
            }
            false
        }
        ...
```

---

## 4. Visual Effects Architecture: Colorful Glowing Mist Wake

### 4.1 Aesthetic Breakdown of `Gemini_Generated_Image_12da9g12da9g12da.jpg`

Inspection of the reference image reveals the following key visual signatures of the bubble slipstream:
1. **Swept-Back Multicolored Comet Tail:**
   The mist is not a cluster of disconnected floating puffs. It begins at the full diameter of the bubble sphere and forms a graceful, tapered slipstream trailing $2.5\times$ to $3.5\times$ the sphere diameter behind it.
2. **Vibrant Iridescent Color Spectrum:**
   - **Inner Core:** Intense, luminous **electric cyan (`#33e0ff`)** transitioning into **vibrant violet-magenta (`#d952ff`)** and a hot **pearlescent pink-white core**.
   - **Outer Rim Ribbons:** Warm **fiery golden-amber (`#ff9426`)** and magenta ribbons peeling off the equatorial slipstream edges.
3. **Longitudinal Anisotropic Speed Streaks:**
   High-speed flow filaments stretched along the direction of motion, giving the optical impression of high-velocity atmospheric shearing.
4. **Floating Iridescent Micro-Bubbles & Star-Sparks:**
   Tiny, sharp translucent bubble rings (soap-bubble SDFs with rainbow rims) and glittering golden and cyan star-sparks floating along the wake.

```
                  =============================================
                  ANATOMY OF THE COLORFUL GLOWING MIST SLIPSTREAM
                  =============================================

                                Fiery Golden-Amber Outer Ribbon
                             /----------------------------------\
                            /     Neon Violet-Magenta Mist       \
         +---------------+ /     /---------------------------\    \
         |               |/     /   Electric Cyan Core Stream \    \
Direction|  BUBBLE       |======|  *     *       o       *    |=====> Trail Direction
of Motion|  SHIELD SPHERE|\     \   (Micro-Bubbles & Sparks)  /    /
         |               | \     \---------------------------/    /
         +---------------+  \     Neon Violet-Magenta Mist       /
                             \----------------------------------/
                                Fiery Golden-Amber Outer Ribbon
```

### 4.2 Why Additive Blending (`AlphaMode::Add`) Is Mathematically Essential

In standard alpha blending (`AlphaMode::Blend`), each fragment evaluates:
$$C_{\text{final}} = C_{\text{src}} \times \alpha + C_{\text{dst}} \times (1 - \alpha)$$
When $\alpha$ is small ($0.15$), each quad attenuates $85\%$ of the background color and replaces it with a dim wisp. On dark terrain or black void, overlapping wisps quickly lose vibrancy and appear like muddy, flat grey-blue smudges.

In **Additive Blending (`AlphaMode::Add`)**:
$$C_{\text{final}} = C_{\text{dst}} + C_{\text{src}} \times \alpha$$
- Additive blending **only adds light ($+$)**. It can **never darken** or obscure the arena floor, units, or structures.
- Where violet-magenta and electric-cyan wisps overlap, their RGB values sum constructively into **superheated, luminous pearlescent white**, exactly matching the glowing core in `Gemini_Generated_Image_12da9g12da9g12da.jpg`!
- Combined with `if (alpha < 0.008) { discard; }` and `depth_bias: -15.0`, additive mist renders smoothly over all geometry without depth-sorting artifacts.

### 4.3 GPU Uniform Layout (Strict 32-Byte std140 Compliance)

WebGPU uniform buffers require 16-byte alignment. We keep `size_of::<MistWakeUniformGpu>() == 32`:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct MistWakeUniformGpu {
    /// Base tint color (RGB) + flow_angle_rad in W.
    pub base_color: Vec4,
    /// Normalized particle lifetime progress [0.0, 1.0].
    pub age: f32,
    /// Particulate kind / mode:
    /// 0 = colorful iridescent nebula wake + slipstream
    /// 1 = golden-magenta thruster overdrive
    /// 2 = iridescent micro-bubble cluster + star-sparks
    pub speed_boosted: u32,
    /// Random noise and rotation seed.
    pub seed: f32,
    /// Aspect ratio of billboard or flow speed factor.
    pub aspect: f32,
}
```

### 4.4 The New `assets/shaders/mist_wake.wgsl` Shader Specification

```wgsl
#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct MistWakeUniform {
    base_color: vec4<f32>,      // rgb = base tint, w = flow_angle_rad
    age: f32,                   // Normalized particle lifetime [0.0, 1.0]
    speed_boosted: u32,         // 0 = colorful nebula, 1 = thruster overdrive, 2 = micro-bubbles
    seed: f32,                  // Random seed
    aspect: f32,                // Aspect ratio / elongation
};

@group(2) @binding(0) var<uniform> mist: MistWakeUniform;

// 2D Hash & Noise primitives
fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d);
}

fn noise2d(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash21(i + vec2<f32>(0.0, 0.0));
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    let shift = vec2<f32>(100.0);
    var pos = p;
    for (var i = 0; i < 4; i++) {
        v += a * noise2d(pos);
        pos = pos * 2.0 + shift;
        a *= 0.5;
    }
    return v;
}

// Inigo Quilez 2-Stage Domain Warping
fn pattern(p: vec2<f32>, time: f32) -> f32 {
    let t = time * 0.20;
    let q = vec2<f32>(
        fbm(p + vec2<f32>(0.0, t)),
        fbm(p + vec2<f32>(5.2, 1.3 - t))
    );
    let r = vec2<f32>(
        fbm(p + 2.8 * q + vec2<f32>(1.7, 9.2 + t * 0.5)),
        fbm(p + 2.8 * q + vec2<f32>(8.3, 2.8 - t * 0.5))
    );
    return fbm(p + 2.8 * r);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Center and rotate coordinates along trail vector
    var uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let angle = mist.base_color.w;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated_p = vec2<f32>(
        uv.x * cos_a - uv.y * sin_a,
        uv.x * sin_a + uv.y * cos_a
    );

    let along_trail = rotated_p.x;   // Negative = forward, Positive = trailing wake
    let across_trail = rotated_p.y;  // Lateral distance from center axis

    let r2 = dot(uv, uv);
    if (r2 > 1.0) {
        discard;
    }

    // 2. Teardrop / Comet-Tail Envelope
    // Broad at front (-0.5), tapering smoothly toward back (+1.0)
    let taper = smoothstep(1.0, -0.4, along_trail);
    let lateral_envelope = exp(-pow(across_trail * (1.2 + along_trail * 1.5), 2.0) * 3.5);
    let tail_envelope = taper * lateral_envelope * (1.0 - r2);

    // 3. 2-Stage Domain-Warped FBM Turbulence
    let noise_coord = vec2<f32>(along_trail * 2.0, across_trail * 3.5) + vec2<f32>(mist.seed * 17.3, mist.seed * 31.7);
    let turbulence = pattern(noise_coord, globals.time);

    // 4. Longitudinal Speed-Streaks / Slipstream Filaments
    let streak_coord = vec2<f32>(along_trail * 1.5 - globals.time * 3.0, across_trail * 12.0);
    let streaks = pow(noise2d(streak_coord), 2.2) * smoothstep(0.1, 0.8, taper);

    // 5. Procedural Iridescent Micro-Bubbles
    // Grid of tiny circular soap bubbles drifting inside the wake
    var bubble_intensity = 0.0;
    var bubble_rainbow = vec3<f32>(0.0);
    if (mist.speed_boosted != 1u) {
        let b_grid = fract(vec2<f32>(along_trail * 3.0, across_trail * 4.0) + vec2<f32>(mist.seed * 5.0, globals.time * 0.1)) - 0.5;
        let b_dist = length(b_grid);
        let b_ring = abs(b_dist - 0.22);
        if (b_ring < 0.06 && b_dist < 0.28) {
            bubble_intensity = smoothstep(0.06, 0.01, b_ring) * (1.0 - mist.age);
            // Rainbow rim tinting
            let hue = fract(b_dist * 8.0 + mist.seed);
            bubble_rainbow = vec3<f32>(
                0.5 + 0.5 * cos(6.28 * (hue + 0.0)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.33)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.67))
            );
        }
    }

    // 6. Glittering Golden & Cyan Star-Sparks
    let spark_noise = noise2d(vec2<f32>(along_trail * 8.0 - globals.time * 5.0, across_trail * 16.0) + mist.seed * 43.1);
    let spark_intensity = smoothstep(0.92, 0.99, spark_noise) * 2.5 * (1.0 - mist.age);

    // 7. Rich Multicolored Iridescent Color Composition (Gemini Reference Image)
    // Core: Electric Cyan + Neon Violet-Magenta + Pearlescent Pink-White
    // Rim: Warm Golden-Amber / Orange-Coral Ribbons
    let col_cyan    = vec3<f32>(0.20, 0.88, 1.0);
    let col_magenta = vec3<f32>(0.85, 0.32, 1.0);
    let col_pink    = vec3<f32>(1.0, 0.82, 0.98);
    let col_amber   = vec3<f32>(1.0, 0.58, 0.15);
    let col_gold    = vec3<f32>(1.0, 0.80, 0.30);

    let lateral_dist = abs(across_trail);
    var core_color = mix(col_cyan, col_magenta, smoothstep(0.2, 0.7, turbulence));
    core_color = mix(core_color, col_pink, smoothstep(0.7, 0.95, turbulence));

    // Outer ribbons get fiery golden-amber
    var final_color = mix(core_color, col_amber, smoothstep(0.25, 0.75, lateral_dist));

    // Speed streaks add intense neon highlights
    final_color += streaks * col_cyan * 1.5;

    // Sparks add gold / cyan glitter
    let spark_col = mix(col_gold, col_cyan, step(0.5, hash21(uv + mist.seed)));
    final_color += spark_intensity * spark_col;

    // Micro-bubbles add rainbow rings
    final_color += bubble_intensity * bubble_rainbow * 2.0;

    // Overdrive mode shifts toward golden-orange thruster fire
    if (mist.speed_boosted == 1u) {
        final_color = mix(final_color, col_amber * 1.8, 0.6);
    }

    // 8. Additive Alpha & Lifetime Fade
    let life_fade = smoothstep(1.0, 0.60, mist.age) * smoothstep(0.0, 0.08, mist.age);
    let density = (turbulence * 0.7 + streaks * 0.3) * tail_envelope * life_fade;
    let alpha = clamp(density * 0.45, 0.0, 0.70);

    if (alpha < 0.008) {
        discard;
    }

    return vec4<f32>(final_color * alpha, alpha);
}
```

### 4.5 Particle Spawning & Lifetime Configuration in `crates/client/src/vfx.rs`

- **Particle Dimensions:**
  - `initial_scale = radius * 2.1` (covering the full diameter of the sphere as seen in the reference image).
  - `target_scale = radius * 3.2` (expanding smoothly backward).
  - `lifetime = 0.75s`.
- **Directional Angle Encoding:**
  Calculate `let flow_angle = forward_dir.z.atan2(forward_dir.x);` and store in `uniform.base_color.w`.
- **Material Registration:**
  Change `alpha_mode` in `MistWakeMaterial` to `AlphaMode::Add`.

---

## 5. Headless Verification & SC2 Matchup Benchmark Tests

We will create a comprehensive, dedicated integration test suite at:
[`crates/sim_core/tests/sc2_battle_benchmarks_tests.rs`](file:///home/cl/projects/space-scuffle/crates/sim_core/tests/sc2_battle_benchmarks_tests.rs)

The suite will include 6 rigorous, headless behavioral tests:

### Test 1: `test_1v1_zealot_vs_marine_melee_pause_and_three_hit_kill`
- Spawns 1 `psi_grey` (Player 0) and 1 `pulse_bot` (Player 1) at point-blank range (distance 0.3).
- Steps simulation tick-by-tick.
- Verifies:
  1. On tick 0, Marine has 45 HP.
  2. Between ticks 0..3, Marine still has 45 HP (Zealot in `Windup` with `damage_point_ticks = 4`).
  3. On tick 4, **Swing 1 lands**: Marine HP drops to 29.
  4. Between ticks 5..25, Marine remains at 29 HP (Zealot in `Backswing` and `Cooldown`).
  5. On tick 26, **Swing 2 lands**: Marine HP drops to 13.
  6. Between ticks 27..47, Marine remains at 13 HP.
  7. On tick 48, **Swing 3 lands**: Marine dies (`is_alive == false`).
  8. Total ticks to kill is exactly $\ge 48\text{ ticks}$ ($1.92+\text{ seconds}$).

### Test 2: `test_equal_cost_1_zealot_vs_2_marines_vs_3_marines`
- **Part A:** 1 Zealot vs. 2 Marines starting at distance 5.0.
  - Verifies Zealot defeats both Marines in $90..140\text{ ticks}$ ($3.6\text{s}..5.6\text{s}$), surviving with $60..90\text{ HP}$.
- **Part B:** 1 Zealot vs. 4 Marines starting at distance 5.0.
  - Verifies 4 Marines defeat the Zealot.

### Test 3: `test_1_zealot_vs_1_marauder_un_kited_duel`
- Spawns 1 `psi_grey` and 1 `rhino_beetle` at distance 5.0.
- Both issue Attack-Target / Attack-Move.
- Verifies Zealot closes in, attacks for $(8 - 1) \times 2 = 14\text{ dmg}$, kills Marauder in $\approx 180..230\text{ ticks}$ ($7.2\text{s}..9.2\text{s}$), and survives with $70..90\text{ HP}$ left.

### Test 4: `test_1_marauder_vs_1_stalker_armored_duel`
- Spawns 1 `rhino_beetle` (Marauder, 125 HP) and 1 `warp_strider` (Stalker, 80 HP, 80 Shields) at distance 5.0.
- Verifies Marauder deals 20 vs. Armored, defeats Stalker in $220..270\text{ ticks}$ ($8.8\text{s}..10.8\text{s}$), and survives with $15..35\text{ HP}$ left.

### Test 5: `test_zergling_swarm_scaling_vs_1_zealot`
- **Part A:** 1 Zealot vs. 4 `razor_mantis` (Zerglings, 35 HP). Verifies Zealot wins.
- **Part B:** 1 Zealot vs. 6 `razor_mantis`. Verifies Zerglings win.

### Test 6: `test_default_battle_sandbox_pacing_and_no_instant_melee_wipe`
- Initializes `SimWorld::new_default_battle_sandbox()`.
- Issues Attack-Move orders for both armies toward map center.
- Verifies:
  1. No units die before tick 40 (movement and approach phase).
  2. At tick 60 (first contact), zero units are dead.
  3. Battle remains active with continuous combat engagements past tick 200 ($8.0+\text{ seconds}$ of active battle).

---

## 6. Implementation Scope for Stage 2

| Step | File | Action | Key Verification |
| :--- | :--- | :--- | :--- |
| **1** | [`crates/client/src/lab_ui.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/lab_ui.rs) | Add `pub battle_tick_accumulator_secs: f32` to `LabState` struct & default. | `cargo check -p client` |
| **2** | [`crates/client/src/scene_controller.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs) | Update `sync_live_battle_system` to consume `time.delta_secs() * speed` in discrete 0.04s increments (max 5 per frame). | Live battle runs at genuine 25 Hz on 144 Hz display. |
| **3** | [`assets/catalog/units.ron`](file:///home/cl/projects/space-scuffle/assets/catalog/units.ron) | 1. `railgun_crawler`: change un-sieged weapon to base 15, bonus armored 10, period 26, `splash: None`, HP 175. Siege mode weapon stays 40+30 splash, period 54.<br>2. `warp_strider`: HP 80, Shields 80, period 34.<br>3. `psi_grey`: period 22, damage point 4, backswing 6.<br>4. `pulse_bot`: period 15, damage point 2, backswing 4.<br>5. `rhino_beetle`: period 27, damage point 2, backswing 4. | Catalog parses cleanly; weapon stats match SC2 LotV. |
| **4** | [`crates/sim_core/src/combat.rs`](file:///home/cl/projects/space-scuffle/crates/sim_core/src/combat.rs) | In `step_entity_weapon`, add `if entity.weapon_cooldown_remaining == 0` check before entering `WeaponPhase::Windup` in `Ready` and `RotatingToTarget`. | Cooldown bypass eliminated on retarget. |
| **5** | [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl) | Implement trail-rotated iridescent shader with electric cyan, neon violet-magenta, golden-amber ribbons, speed-streaks, micro-bubbles, and star-sparks. | Shader compiles cleanly with naga. |
| **6** | [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs) | 1. Set `alpha_mode: AlphaMode::Add` on `MistWakeMaterial`.<br>2. Pass `flow_angle` in `uniform.base_color.w`.<br>3. Scale particles to `radius * 2.1` initial, `radius * 3.2` target, lifetime `0.75s`. | Colorful glowing slipstream rendered behind moving units. |
| **7** | [`crates/sim_core/tests/sc2_battle_benchmarks_tests.rs`](file:///home/cl/projects/space-scuffle/crates/sim_core/tests/sc2_battle_benchmarks_tests.rs) | Create the 6 headless SC2 matchup benchmark integration tests. | All 6 tests pass cleanly. |
| **8** | Workspace Quality Gate | Run cargo tests, clippy, and fmt. | `cargo test --workspace`<br>`cargo clippy --workspace --all-targets -- -D warnings`<br>`cargo fmt --all -- --check` |

---
*Chartered by: Principal Combat Balance & Visual Effects Architect (Stage 1 Blueprint)*
