# Plan 18: "Sweating" Bubbles, Aerodynamic Air-Suction Mist Wake, Volumetric Combat VFX Shaders, and Smooth Anti-Vibration Convergence Physics

**Document Status:** Architectural Blueprint (Stage 1)  
**Author:** Principal Visual Effects & Simulation Physics Architect  
**Target Crates:** `crates/client`, `crates/sim_core`, `assets/shaders`  
**Binding Standards:** [`AGENTS.md`](file:///home/cl/projects/space-scuffle/AGENTS.md) (Textbook-Grade Engineering, Zero Placeholders, Strict Invariants)  
**Reference Artworks:**
- [`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg) — High-energy laser beams, iridescent aerodynamic mist wakes, trailing particle streams.
- [`damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg`](file:///home/cl/projects/space-scuffle/tmp/damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg) — Bubble under fire, shield ripple arcs, impact plasma starbursts, crystalline glass shatter explosion.

---

## 1. Executive Summary & Problem Diagnosis

### 1.1 The User's Feedback & Visual Objectives
Following the implementation of Plan 17, combat pacing and pathfinding improved significantly. However, critical evaluation of live battle and visual lab footage revealed three major architectural defects:

1. **Mist Wake Movement:**
   > *"the mist is much better, but I think it would look much better, if mist just trailed behind. Currently the units push the mist away (i.e. the particles have a speed). What would look much better: if the bubble just left mist behind. It should have an air suction effect (because of the movent), but otherwise the mist is just an effect, because every bubble 'sweats' mist particles. Please try this."*
2. **Combat Visuals:**
   > *"Then take a good look at the fight animations. They look horribly primitive. (lasers, disks,..) We can do much better."*
3. **Unit Convergence & Push Vibration:**
   > *"The path algorithm is now far better. But we have a vibrating/shaking effect, when bubbles are pushed. Please animate a few units converging to another unit. Youl will notice, that some units change their positions too rapidly. I don't know if for instance a unit is allowed to ever move faster than its configured speed when pushed. (I don't think so). Summarized: currently the movements are not smooth when multiple units converge."*

To elevate Space Scuffle to textbook-grade AAA fidelity matching the reference concept art and competitive RTS standards, this blueprint establishes three comprehensive, mathematically rigorous subsystems:
1. **The "Sweating Bubble + Aerodynamic Air-Suction" Fluid Model (Part 1):** Eliminates the "rocket exhaust" backward propulsion bug. Simulates natural omnidirectional surface perspiration across all bubbles, while moving bubbles leave stationary mist in world space that is pulled inward and slightly forward by the aerodynamic low-pressure suction pocket trailing behind the sphere.
2. **Volumetric Procedural Combat VFX Engine (`combat_fx.wgsl`) (Part 2):** Completely replaces primitive Bevy geometric primitives (unlit cylinders, spheres, and torus rings) with a custom WebGPU/WGSL additive volumetric shader supporting 6 distinct procedural energy modes: high-energy continuous lasers with dual flares, supersonic railgun tracers with Mach shock cones, 3-phase domain-warped lightning arcs, sweeping crescent psi-blades, comet-tail plasma projectiles, and 8-point impact starbursts with refractive shockwaves.
3. **Smooth Convergence, Push Speed Clamping, and Anti-Vibration Physics (Part 3):** Resolves the 4 root causes of unit shaking/vibration during multi-unit convergence by introducing Baumgarte push relaxation (`PUSH_RELAXATION = 0.35`), strict per-tick speed caps, combat engagement anchoring against friendly pushes, deterministic tangent deadzones, and client-side inter-tick transform interpolation.

---

### 1.2 Forensic Analysis: Why Current Mist Wake Looks Like "Pushing Mist Away"

A forensic inspection of [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs), [`crates/client/src/scene_controller.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs), and [`crates/sim_core/src/world.rs`](file:///home/cl/projects/space-scuffle/crates/sim_core/src/world.rs) identified three interrelated mathematical and kinematic causes for the unnatural "thrusting / pushing away" appearance:

```
CURRENT (FLAWED) ROCKET EXHAUST MODEL:
======================================
     Bubble Movement ---> [ +forward_dir ]
             ( O )  ======>   [ +V_bubble ]
               |
               +---> Particles spawned at rear anchor with BACKWARD velocity:
                     v_particle = -forward_dir * (speed * speed_mult)
                     v_particle shoots AWAY from rear like a rocket thruster!
                     Scale EXPANDS: initial_scale (2.1R) -> target_scale (3.2R)
                     Result: Looks like an exhaust plume pushing gas backwards into empty space.

PHYSICALLY INSPIRED "SWEATING BUBBLE + AIR SUCTION" MODEL:
==========================================================
     Bubble Movement ---> [ +forward_dir ]
            (  O  ) ======>   [ +V_bubble ]
           /   |   \
      Vapor "sweats" from entire sphere surface into world space.
      As the bubble leaves the vapor behind at world coordinate P_0:
      Low-pressure cavity in sphere's wake sucks mist INWARD toward the centerline:
      v_initial = -offset_lateral / 0.32s + forward_dir * (0.22 * V_world)
      Scale CONTRACTS: initial_scale (1.90R) -> target_scale (0.85R)
      Air drag brings vapor to a dead halt: v_particle *= exp(-3.8 * dt)
      Result: Sleek, necked aerodynamic suction wake left stationary behind moving bubble!
```

#### Flaw 1: Active Backward Velocity Ejection
In `crates/client/src/vfx.rs:325`, `drift_velocity` is calculated as:
```rust
let drift_velocity = trail_dir * (speed * speed_mult) + lateral_jitter * 0.1;
```
Because `trail_dir = -forward_dir`, the mist particles are propelled backward away from the bubble with speed proportional to the unit's speed. Instead of the bubble simply traversing through space and leaving sweat vapor behind, the bubble actively ejects particulate matter out of its tail, creating the visual illusion of a rocket engine or thruster plume.

#### Flaw 2: Scale Expansion vs. Suction Contraction (Neck Inversion)
In `crates/client/src/vfx.rs:295-296`, the scale transition is:
```rust
let initial_scale = radius * 2.1;
let target_scale = radius * 3.2;
```
As particles age from `age = 0.0` to `1.0`, their billboard size expands from $2.1R$ to $3.2R$. Real aerodynamic fluid wakes behind spheres exhibit **suction necking**: immediately behind the separation boundary, the streamlines converge toward the low-pressure centerline axis, pinching the wake into a tapered hourglass before dissipating. Expanding the particle scale from $2.1R$ to $3.2R$ creates an outward billow that visually mimics explosive exhaust rather than an aerodynamic vacuum slipstream.

#### Flaw 3: Simulation Tick Displacement vs. World Velocity Discrepancy
In `crates/sim_core/src/world.rs:2772`, `entity.velocity` is stored as fixed-point tick displacement in world units per tick (ranging from $\approx 0.09$ to $0.15$). In `crates/client/src/scene_controller.rs:561`:
```rust
let moving = instance.velocity[0].abs() + instance.velocity[1].abs() > 0.05;
```
The threshold $0.05$ is barely above stationary for units moving at $0.09\text{ units/tick}$, and when `instance.velocity` is passed directly into `emit_mist_wake`:
```rust
emit_mist_wake(..., Vec2::new(instance.velocity[0], instance.velocity[1]), ..., false);
```
the speed calculation inside `emit_mist_wake` (`speed = velocity.length()`) evaluates to $\approx 0.10$, whereas the actual physical movement speed in world space per second at $25\text{ Hz}$ is:
$$v_{\text{world}} = v_{\text{tick}} \times 25.0 \approx 2.25 \dots 3.75\text{ units/second}$$
This created a severe velocity-scaling mismatch between the Visual Lab (where `lab_state.speed` was passed as world units per second, e.g. $2.5$) and Live Battle (where velocity was passed as tick displacement, e.g. $0.10$).

---

### 1.3 Forensic Analysis: Why Fight Animations Look "Horribly Primitive"

Comparing the current implementation in `crates/client/src/vfx.rs` against the reference images ([`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg) and [`damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg`](file:///home/cl/projects/space-scuffle/tmp/damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg)) demonstrates why the battle looks rudimentary:

1. **Hitscan & Laser Beams (`tracer_mesh` & `beam_mesh`):**
   - **Current:** Spawns a 3D solid unlit cylinder (`Cylinder::new(0.03, 1.0)` or `Cylinder::new(0.05, 1.0)`) with a flat `StandardMaterial` color (`blue_beam_mat` / `red_beam_mat`).
   - **Why It Looks Primitive:** It looks like a stiff plastic stick or solid rod floating in air. There is no white-hot core, no soft exponential plasma halo, no high-frequency energy fluctuation, and no bright muzzle flash or target impact starburst!
   - **Reference Art Standard:** Continuous lasers must have an ultra-intense white-hot core ($E \approx 10.0$), surrounded by an iridescent additive plasma corona, longitudinal helical energy ripples, a bright circular muzzle aperture flare at the shooter, and a violent plasma splash with spark rays at the impact point.

2. **Melee Slashes (`melee_slash_mesh`):**
   - **Current:** Spawns a full 360-degree donut ring (`Torus::new(0.04, 0.8)`) with flat `StandardMaterial`.
   - **Why It Looks Primitive:** Instead of a sharp, dynamic crescent arc swept out by a psi-blade or claw, it renders an entire solid circular ring/disk around the target. It looks like an out-of-place rubber hula-hoop or flat donut!
   - **Reference Art Standard:** A high-speed razor-sharp crescent arc spanning $\approx 120^\circ$ ($\theta \in [-1.0, +1.0]$ radians), featuring a blinding white leading edge, a trailing turbulent energy veil with FBM noise, and directional spark embers flying tangentially along the slash vector.

3. **Traveling Projectiles (`projectile_mesh`):**
   - **Current:** Spawns a low-tessellation sphere (`Sphere::new(0.18).mesh().uv(10, 8)`) with flat `StandardMaterial`.
   - **Why It Looks Primitive:** A tiny solid 3D marble moving in a straight line. There is no comet tail, no luminous corona, and no radiant wake.
   - **Reference Art Standard:** A blazing plasma comet featuring a white-hot nucleus, a swirling corona, and a tapering tail with micro-sparks trailing behind along the velocity vector.

4. **Death Shatter & Explosions (`shard_mesh`):**
   - **Current:** Spawns 16 identical rectangular bricks (`Cuboid::new(0.15, 0.30, 0.08)`) with standard PBR material.
   - **Why It Looks Primitive:** There is no central detonation flash or shockwave. Rectangular boxes tumbling outward do not evoke shattering glass or bursting soap bubbles.
   - **Reference Art Standard:** (Right side of [`damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg`](file:///home/cl/projects/space-scuffle/tmp/damaged bubbles-Gemini_Generated_Image_mqt7trmqt7trmqt7.jpg)): A blinding golden-white core starburst with radial rays, an expanding refractive vapor ring, and sharp triangular/wedge-shaped crystalline shards scattering outward with glowing iridescent refraction.

---

### 1.4 Forensic Analysis: The 4 Root Causes of Convergence Vibration & Rapid Shaking

When multiple units converge upon a single target, players observe rapid vibration and excessive movement speeds. Investigation of `crates/sim_core/src/world.rs:1940-2085` and `crates/sim_core/src/steering.rs:200-433` revealed 4 distinct bugs:

```
THE 4-POINT CONVERGENCE VIBRATION LOOP:
======================================================================================
1. Unbounded Cumulative Push Displacement:
   Pairwise overlaps are applied consecutively without clamping.
   Unit (max_speed=0.09) is shoved by 4 neighbors by 0.45 units (5x max_speed)!
      |
      v
2. Attack-Engaged Frontline Shoved by Rear Allies:
   Frontline unit halts to attack (vel=0). Rear allies with vel > 0 invoke Rule 3,
   shoving the firing unit laterally/forward into the enemy!
      |
      v
3. Enemy Repulsion & Sign-Flipping Perpendicular Tangents:
   Enemy exerts hard reciprocal repulsion. Cross product cross < 0 flips sign across 0,
   reversing tangential avoidance +90 deg <---> -90 deg every tick!
      |
      v
4. Unsmoothed 25 Hz Snapshot Render Snapping:
   Client snaps Transform::translation = pos every frame without sub-tick lerp,
   amplifying 25 Hz positional reversals into 144 Hz visual screen judder!
```

---

## 2. Mathematical & Fluid Model: Sweating Bubbles & Aerodynamic Air-Suction Wake

```
               AERODYNAMIC SUCTION WAKE FLUID GEOMETRY
========================================================================
       Bubble Front           Equatorial Sweat           Wake Cavity (Low Pressure)
       (Stagnation)              Boundary               (Inward & Forward Suction)
          +---+                 +--------+                  \      |      /
        /       \             /            \                 \     |     /
       |  Unit   | --------> (    Bubble    )      ==>        \  - - - -/
        \  Dir  /             \            /                   \   P_c   /
          +---+                 +--------+                      v  |    v
                                                                 Centerline
                                      |                             |
                                      +--- Vapor leaves bubble      |
                                           at (x_0, y_0, z_0).      |
                                           Suction pulls inward <---+
                                           towards Centerline Axis.
```

### 2.1 Principle A: Omnidirectional Surface Sweating
Every mobile bubble in Space Scuffle is an energy-sheathed crystalline/fluid orb. Regardless of whether it is stationary or in transit, the bubble membrane continuously "sweats" luminescent vapor into the surrounding environment.

1. **Stationary State ($v_{\text{world}} < 0.05$):**
   - Sweat rate: Low frequency (e.g. 1 emission every 6 ticks / 0.24s).
   - Spawn distribution: Uniform random distribution across the entire spherical shell:
     $$\vec{P}_{\text{spawn}} = \vec{P}_{\text{bubble}} + R \cdot (\sin\theta\cos\phi\,\hat{x} + \cos\theta\,\hat{y} + \sin\theta\sin\phi\,\hat{z})$$
   - Velocity: Zero horizontal propulsion ($\vec{v}_{xy} = 0$). Gentle upward thermal buoyancy drift:
     $$\vec{v}_{\text{sweat}} = (0.0, 0.12, 0.0)\text{ units/second}$$
   - Scale profile: Slight expansion from $1.55R \to 1.35R$ with soft alpha dissipation over $1.2\text{s}$.
   - Result: Idling units possess an ethereal, breathing, iridescent atmospheric haze that clearly conveys that mist is an organic property of the bubble itself.

2. **Moving State ($v_{\text{world}} \ge 0.05$):**
   - Sweat rate: Proportional to speed (emissions every 2–3 ticks).
   - Emitters distribute vapor across the entire frontal and equatorial periphery of the sphere, not just a single rear tail point.

---

### 2.2 Principle B: Aerodynamic Suction & Wake Necking Fluid Dynamics

When a sphere of radius $R$ moves through a fluid/atmospheric medium with velocity $\vec{V} = v_{\text{world}}\,\hat{u}_{\text{forward}}$, fluid separation occurs near the equator ($\theta \approx 82^\circ$). Behind the sphere, a turbulent low-pressure recirculating cavity (the toroidal vortex bubble) is established.

#### 1. Inward Centerline Suction Force
Any particulate matter released into the fluid near the separation zone experiences an inward radial pressure gradient:
$$\nabla P_{\text{radial}} \propto -\frac{\vec{r}_{\perp}}{R}$$
where $\vec{r}_{\perp}$ is the lateral displacement perpendicular to the movement trajectory:
$$\vec{r}_{\perp} = \vec{P}_{\text{spawn}} - (\vec{P}_{\text{bubble}} + (\vec{P}_{\text{spawn}} \cdot \hat{u}_{\text{forward}})\hat{u}_{\text{forward}})$$
To simulate this suction without expensive Navier-Stokes grid solvers, we endow freshly spawned mist particles with an initial inward suction velocity directed toward the wake centerline:
$$\vec{v}_{\text{suction, lateral}} = -\frac{\vec{r}_{\perp}}{\tau_{\text{suction}}}, \quad \text{where } \tau_{\text{suction}} \approx 0.32\text{ seconds}$$

#### 2. Forward Slipstream Entrainment
In addition to inward suction, viscous shear and the traveling pressure deficit pull the wake forward in the direction of the bubble:
$$\vec{v}_{\text{suction, longitudinal}} = \hat{u}_{\text{forward}} \cdot (0.22 \cdot v_{\text{world}})$$
Crucially, **the backward velocity component is strictly zero**:
$$\vec{v}_{\text{backward}} \equiv \vec{0}$$
The mist is **not pushed away**; it is released into world space and gently drawn along behind the bubble!

#### 3. Aerodynamic Wake Necking (Scale Contraction)
Because the surrounding fluid rushes in to fill the cavity, the effective diameter of the vapor packet contracts as it enters the neck of the wake:
$$S(t) = S_{\text{initial}} + (S_{\text{target}} - S_{\text{initial}}) \cdot \text{progress}$$
where:
$$S_{\text{initial}} = 1.90 \cdot R, \quad S_{\text{target}} = 0.85 \cdot R$$
This $1.90R \to 0.85R$ contraction produces the iconic aerodynamic suction waist visible in fluid dynamics wind tunnels and the reference concept art.

#### 4. Exponential Atmospheric Drag
Once released, the mist particles encounter the stationary ambient atmosphere and decelerate rapidly:
$$\vec{v}(t + \Delta t) = \vec{v}(t) \cdot e^{-3.8\,\Delta t}$$
Within $0.4$ seconds, all relative motion halts, leaving the mist pinned at its world-space coordinate while gracefully fading out.

---

### 2.3 Coordinate Transformations: Live Battle vs. Visual Lab

To ensure identical, breathtaking visual fidelity across all game modes, the fluid model supports two kinematic frames of reference:

```rust
// Kinematic Frame of Reference Dispatch
if stationary_orb {
    // VISUAL LAB MODE:
    // The hero showcase bubble is anchored at fixed coordinate P_lab = (0, 0, 0),
    // but the user has configured lab_state.speed (e.g. 3.0 units/sec).
    // In this reference frame, the bubble is stationary and the relative "wind"
    // blows backward along -forward_dir.
    //
    // Emitters spawn across the wide equatorial ring:
    let lateral_offset = right_dir * (radius * 0.80 * right_jitter)
                       + up_dir * (radius * 0.60 * up_jitter);
    let spawn_pos = bubble_pos + lateral_offset;
    
    // Suction draws particles INWARD towards the rear axis while wind carries them backward:
    let inward_pull = -lateral_offset * 2.4;
    let backward_wind = trail_dir * (speed * 0.85);
    let particle_velocity = backward_wind + inward_pull;
    
    // Scale contracts to form sleek tapered tail:
    let initial_scale = radius * 1.95;
    let target_scale = radius * 0.85;
} else {
    // LIVE BATTLE MODE:
    // Bubble moves through authoritative world space with velocity V_world.
    // Particles are deposited at world coordinates and left behind!
    let lateral_offset = right_dir * (radius * 0.75 * right_jitter)
                       + up_dir * (radius * 0.55 * up_jitter);
    let spawn_pos = bubble_pos + lateral_offset + trail_dir * (radius * 0.25);
    
    // Zero backward push! Only inward centerline suction + slight forward entrainment:
    let inward_suction = -lateral_offset / 0.32;
    let forward_pull = forward_dir * (speed * 0.22);
    let particle_velocity = inward_suction + forward_pull;
    
    // Scale contracts into suction waist:
    let initial_scale = radius * 1.90;
    let target_scale = radius * 0.85;
}
```

---

## 3. Visual Architecture: Volumetric `combat_fx.wgsl` Shader & Procedural SDF Engine

To replace all primitive cylinders, spheres, and donuts with cinematic visual effects, we introduce a dedicated procedural additive shader: `assets/shaders/combat_fx.wgsl`, paired with `CombatFxMaterial` and `CombatFxUniformGpu`.

### 3.1 GPU Uniform Layout & Strict 32-Byte Alignment
In compliance with WebGPU std140 uniform buffer layout rules and AGENTS.md standards, `CombatFxUniformGpu` is strictly packed into exactly 32 bytes with zero padding bugs:

```rust
// PERFORMANCE RATIONALE:
// Why: WebGPU uniform buffers require strict 16-byte alignment and explicit std140 packing.
// What: Fields total exactly 32 bytes (16B Vec4 + 4x 4B primitive fields).
// Trade-off: Explicit manual packing ensures zero runtime WebGPU alignment validation panics.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable, ShaderType)]
pub struct CombatFxUniformGpu {
    /// Primary HDR color (RGB) + Secondary parameter / angle (W).
    pub primary_color: Vec4,
    /// Normalized effect lifetime progress [0.0, 1.0].
    pub age: f32,
    /// Effect classification kind:
    /// 0 = FX_LASER_BEAM (Continuous high-energy laser with muzzle & impact flares)
    /// 1 = FX_HITSCAN_TRACER (Supersonic kinetic/pulse tracer with Mach shock rings)
    /// 2 = FX_LIGHTNING_ARC (3-phase domain-warped jagged electric arc)
    /// 3 = FX_MELEE_SLASH (Sweeping crescent psi-blade with trailing energy veil)
    /// 4 = FX_PLASMA_PROJECTILE (Blazing comet-tail plasma orb)
    /// 5 = FX_IMPACT_STARBURST (8-pointed radial starburst & refractive shockwave)
    pub fx_kind: u32,
    /// Randomization seed for turbulence, branching, and sparks.
    pub seed: f32,
    /// Geometric aspect ratio (length / width for ribbons, radius for discs).
    pub aspect: f32,
}
```

---

### 3.2 Coordinate Systems: Ribbon Quad Space vs. Polar Crescent Space

Every combat effect is rendered on an optimized geometric quad billboard or ribbon:

1. **Beam / Tracer / Lightning Ribbon Quads (`fx_kind` 0, 1, 2):**
   - Quad geometry: Centered at `(p1 + p2) * 0.5`, length equals `length = (p2 - p1).length()`, width is specified per effect type ($0.55 \dots 0.95$).
   - Alignment: Quad's local $Y$ axis points along `dir = (p2 - p1).normalize()`, while its normal faces the camera.
2. **Melee Slash Tilted Quads (`fx_kind` 3):**
   - Quad geometry: Horizontal square quad of size $2.6 \cdot R$ positioned at the contact boundary, tilted $35^\circ$ toward the camera for maximum readability from RTS overhead perspective.
   - Polar mapping: $r = \sqrt{x^2 + y^2}, \quad \theta = \operatorname{atan2}(y, x) - \text{slash\_angle}$.
3. **Plasma Projectile & Starburst Quads (`fx_kind` 4, 5):**
   - Camera-facing billboard quad in normalized coordinates $p \in [-1.0, +1.0]^2$.

---

### 3.3 The 6 Procedural Modes in `combat_fx.wgsl`

- **Mode 0 (`FX_LASER_BEAM`):** High-energy plasma laser on a beam-aligned ribbon quad (width 0.65). Core: $\exp(-u^2 \times 110.0) \times 3.5$; Corona: $\exp(-u^2 \times 14.0) \times 1.6$; Helical energy ripple: $\sin(v \cdot \text{aspect} \cdot 4.0 - t \cdot 45.0)$; Circular muzzle flare at $v \approx -0.90$ and impact starburst at $v \approx +0.90$.
- **Mode 1 (`FX_HITSCAN_TRACER`):** Supersonic railgun tracer on ribbon quad (width 0.55). High-velocity Mach bolt sweeping along $v$ from $0.0 \to 1.0$, accompanied by Mach shock rings and a crisp terminal spark splash.
- **Mode 2 (`FX_LIGHTNING_ARC`):** 3-phase domain-warped jagged electric arc on ribbon quad (width 0.95). 3 braided lightning filaments modulated by 1D Perlin noise and high-frequency $60\text{ Hz}$ dielectric discharge strobe flicker.
- **Mode 3 (`FX_MELEE_SLASH`):** Sweeping crescent psi-blade arc on tilted 3D quad ($2.6R$). Replaces `Torus` donut completely! Polar angle strictly masked to $\theta \in [-1.35, +1.35]$ (zero $360^\circ$ circle), razor-sharp blade edge at $r \approx 0.72$, and trailing FBM plasma veil with speed lines.
- **Mode 4 (`FX_PLASMA_PROJECTILE`):** Blazing comet-tail plasma orb on velocity-aligned quad ($1.1 \times 0.65$). Blazing white-hot core at $v \approx +0.30$, outer iridescent halo, and tapering tail with turbulent micro-sparks streaming behind.
- **Mode 5 (`FX_IMPACT_STARBURST`):** 8-pointed radial diffraction rays ($|\cos(4\theta)|^8$), central detonation flash ($E \approx 6.0$), and expanding refractive shockwave ring ($r_{\text{wave}} = 0.0 \to 0.85$). Spawned on impacts and at the center of `spawn_death_shatter` alongside 16 crystalline glass shards.

---

## 4. Part 3: Smooth Convergence, Push Speed Clamping, and Anti-Vibration Physics

To permanently eradicate unit shaking, violent jitter, and unnatural velocity spikes when multiple units converge, this section establishes four architectural physics guarantees in `sim_core` and `client`.

### 4.1 Root Cause 1: Unbounded Cumulative Push Displacement & 1-Tick Full Overlap Snap

#### The Problem
In `crates/sim_core/src/steering.rs:294-433`, `compute_push_displacement` computes overlap as:
$$\text{overlap} = (R_a + R_b) - \|\vec{P}_a - \vec{P}_b\|$$
and returns $100\%$ of this overlap (e.g. `perp * overlap` or `sep_dir * (overlap * 0.5)`) to be resolved in a **single tick ($0.04\text{s}$)**.
In `crates/sim_core/src/world.rs:2057-2084`, `resolve_collisions_and_push` pushes every pairwise `(id_a, delta_a)` and `(id_b, delta_b)` into a flat `Vec<(EntityId, SimVec2)>`. Then, it applies all of them sequentially:
```rust
for (id, delta) in displacements {
    if let Some(ent) = self.entities.get_mut(&id) {
        let new_pos = ent.position + delta;
        ...
        ent.position = new_pos;
    }
}
```
If a unit is surrounded by 4 converging allies, each with overlap $0.10$, the unit receives 4 separate displacement pushes in a single tick:
$$\Delta \vec{P}_{\text{total}} = \vec{\delta}_1 + \vec{\delta}_2 + \vec{\delta}_3 + \vec{\delta}_4 \approx 0.40\text{ units}$$
Since the unit's configured `max_speed` is only $0.09\text{ units/tick}$, the unit is forcefully teleported by **$4.4\times$ its maximum speed** in a single tick! On the subsequent tick, the unit's steering logic detects that it is off-course and drives it back toward the target ($\vec{v} \approx +0.09$). This causes violent back-and-forth oscillation ($+0.40 \leftrightarrow -0.09$) every tick—manifesting visually as high-frequency vibration and shaking!

#### The Architectural Solution
1. **Baumgarte Relaxation Damping (`PUSH_RELAXATION = 0.35`):**
   Instead of snapping $100\%$ of overlap in 1 tick, apply a stable relaxation factor:
   $$\text{effective\_overlap} = \text{overlap} \times \text{PUSH\_RELAXATION}$$
   where $\text{PUSH\_RELAXATION} = \text{SimScalar::from\_f32\_lossy}(0.35)$.
   Overlapping pairs converge smoothly over $\approx 3$ ticks without overshoot, preventing kinematic bouncing.
2. **Per-Entity Net Push Accumulation:**
   In `resolve_collisions_and_push`, accumulate a single net displacement vector per entity using `BTreeMap<EntityId, SimVec2>`:
   ```rust
   let mut net_pushes: BTreeMap<EntityId, SimVec2> = BTreeMap::new();
   // For each pairwise collision:
   *net_pushes.entry(id_a).or_insert(SimVec2::ZERO) += delta_a;
   *net_pushes.entry(id_b).or_insert(SimVec2::ZERO) += delta_b;
   ```
3. **Strict Speed Cap Invariants:**
   Before applying net pushes, enforce two strict mathematical bounds:
   - **Bound A (Push Velocity Limit):** Being pushed by neighbors can never impart more than $0.65\times$ of the unit's max speed:
     $$\|\vec{\delta}_{\text{net}}\| \le \text{max\_speed} \times 0.65$$
   - **Bound B (Total Tick Displacement Limit):** The combined sum of intentional steering movement ($\vec{v}_{\text{steer}}$) and external push displacement ($\vec{\delta}_{\text{net}}$) can **never** exceed the unit's authoritative `max_speed`:
     $$\|\vec{P}_{\text{final}} - \vec{P}_{\text{start\_of\_tick}}\| \le \text{max\_speed}$$
     If the sum exceeds `max_speed`, the net vector is normalized and clamped to `max_speed`:
     $$\vec{P}_{\text{final}} = \vec{P}_{\text{start\_of\_tick}} + (\vec{P}_{\text{final}} - \vec{P}_{\text{start\_of\_tick}}).\text{clamp\_length\_max}(\text{max\_speed})$$

---

### 4.2 Root Cause 2: Front-Line Attacking Units Shoved by Rear Moving Allies

#### The Problem
In SC2 and competitive RTS combat, units form a concave firing line. When front-line melee or ranged units arrive at weapon range ($d \le R_{\text{weapon}}$), their velocity drops to zero ($\vec{v} = \vec{0}$) as they begin their attack cycle (`WeaponPhase::Windup` / `RotatingToTarget` / `Backswing`).
In `crates/sim_core/src/steering.rs:369-401`:
```rust
// Rule 3: Idle friendly units yield sideways perpendicular to moving unit's velocity
if b.vel.length_sq() == SimScalar::ZERO
    && a.vel.length_sq() > SimScalar::ZERO
    && a.push_priority >= b.push_priority
{
    ...
    return (SimVec2::ZERO, perp * overlap);
}
```
Because the front-line attacking unit has `b.vel == 0` and equal `push_priority`, a friendly unit arriving from behind (with `a.vel > 0`) treats the attacking unit as an "idle" unit! It shoves the attacking unit sideways or forward into the enemy!
The enemy unit then immediately pushes the front-line unit back on the following tick via Rule 1 (Enemy Repulsion). This creates a 2-tick ping-pong oscillation loop where attacking units are endlessly jostled and shaken by their own reinforcements!

#### The Architectural Solution
An actively attacking unit is not "idle"—it is an anchored combatant.
In `crates/sim_core/src/world.rs:1997-2006`:
```rust
let is_actively_engaged = entity_a.order_queue.front().is_some_and(|o| {
    matches!(o, UnitOrder::AttackTarget { .. } | UnitOrder::AttackMove { .. })
}) && (
    entity_a.current_target.is_some() ||
    !matches!(entity_a.weapon_phase, WeaponPhase::Ready)
);
```
Treat any actively engaged unit as anchored against friendly pushes of equal priority:
```rust
let cannot_be_pushed_a = entity_a.cannot_be_pushed() || is_actively_engaged;
```
Reinforcements arriving from behind cannot shove the firing line; instead, they smoothly steer *around* the frontline via tangential avoidance to form a wider concave arc.

---

### 4.3 Root Cause 3: Sign-Flipping Perpendicular Wedge Direction

#### The Problem
In `crates/sim_core/src/steering.rs:200-205` and `crates/sim_core/src/steering.rs:380, 424`:
```rust
let cross = goal_dir.x * to_blocker.y - goal_dir.y * to_blocker.x;
let steer_left = if cross.abs() > SimScalar::from_f32_lossy(0.01) {
    cross < SimScalar::ZERO
} else {
    unit_id.0 % 2 == 0
};
```
When two units approach near head-on, `cross` hovers around zero (e.g. $+0.012$ on tick $t$, $-0.008$ on tick $t+1$). This causes `steer_left` to flip between `true` and `false` every tick, violently flipping the perpendicular avoidance tangent $\vec{\tau} = (-y, x) \leftrightarrow (y, -x)$ by $180^\circ$!
Furthermore, in Rule 4 (`steering.rs:427-428`):
```rust
let wedge_dir_a = (sep_dir + perp * SimScalar::from_f32_lossy(0.75)).normalize_or_zero();
```
The tangential wedge component was weighted heavily at $0.75$, causing moving units to forcefully whip each other laterally upon collision.

#### The Architectural Solution
1. **Hysteresis Deadzone with Deterministic Tie-Breaker:**
   Increase the neutral deadzone to $0.05$:
   ```rust
   let cross_threshold = SimScalar::from_f32_lossy(0.05);
   let steer_left = if cross.abs() > cross_threshold {
       cross < SimScalar::ZERO
   } else {
       // Deterministic tie-breaker based on persistent entity IDs
       unit_id.0 < blocker_id.0
   };
   ```
2. **Predominantly Radial Repulsion for Moving Allies (Rule 4):**
   Reduce the tangential wedge component from $0.75$ to $0.25$:
   ```rust
   let wedge_dir_a = (sep_dir + perp * SimScalar::from_f32_lossy(0.25)).normalize_or_zero();
   let wedge_dir_b = (-sep_dir - perp * SimScalar::from_f32_lossy(0.25)).normalize_or_zero();
   ```
   Converging moving units slide past each other smoothly rather than violently ricocheting.

---

### 4.4 Root Cause 4: Visual Render Transform Smoothing Between 25 Hz Ticks

#### The Problem
`sim_core` executes deterministically at $25\text{ Hz}$ ($\Delta t = 0.04\text{s}$). Bevy renders frames at monitor refresh rates ($60\text{ Hz} \dots 144+\text{ Hz}$).
In `crates/client/src/scene_controller.rs:556`:
```rust
transform.translation = pos;
```
Every render frame, the visual entity's position is hard-snapped to the latest simulation snapshot coordinate. When a unit changes direction or steps forward at $25\text{ Hz}$, the position stays static for 3-5 render frames and then instantly jumps. This discrete step function makes small physical adjustments appear as rapid visual judder and shaking on high-refresh displays.

#### The Architectural Solution
In `crates/client/src/scene_controller.rs:552-560`, apply frame-rate independent exponential smoothing between simulation ticks:
```rust
let current_trans = transform.translation;
let dist = current_trans.distance(pos);

// Snap on initial spawn or large teleportations (e.g. blink)
if dist > 3.0 || dist < 0.0001 {
    transform.translation = pos;
} else {
    // Exponential smoothing factor (~22.0 s^-1 gives responsive, butter-smooth 144Hz tracking)
    let lerp_factor = (time.delta_secs() * 22.0).min(1.0);
    transform.translation = current_trans.lerp(pos, lerp_factor);
}
```

---

## 5. Detailed WGSL Shader Implementation (`assets/shaders/combat_fx.wgsl`)

Below is the complete, production-grade WGSL code for `assets/shaders/combat_fx.wgsl`:

```wgsl
// assets/shaders/combat_fx.wgsl
// Textbook-Grade Procedural Volumetric Combat VFX Shader for Space Scuffle.
// Renders 6 distinct combat modes:
// Mode 0: High-Energy Continuous Laser Beam with Muzzle & Impact Flares
// Mode 1: Supersonic Railgun / Pulse Tracer with Mach Cones
// Mode 2: 3-Phase Domain-Warped Jagged Lightning Arc
// Mode 3: Sweeping Crescent Psi-Blade Melee Energy Slash
// Mode 4: Blazing Comet-Tail Plasma Projectile Orb
// Mode 5: 8-Pointed Detonation Starburst & Refractive Shockwave Ring

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct CombatFxUniform {
    primary_color: vec4<f32>, // rgb = HDR team color, w = secondary hue or orientation angle
    age: f32,                 // Normalized progress [0.0, 1.0]
    fx_kind: u32,             // 0=Laser, 1=Tracer, 2=Lightning, 3=Melee, 4=Projectile, 5=Starburst
    seed: f32,                // Random seed
    aspect: f32,              // Aspect ratio (length / width)
};

@group(2) @binding(0) var<uniform> fx: CombatFxUniform;

// Fast Pseudo-Random Noise Primitives
fn hash11(p: f32) -> f32 {
    let q = fract(p * 0.1031);
    let d = q * (q + 33.33);
    return fract(d * (d + d));
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d);
}

fn noise1d(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(hash11(i), hash11(i + 1.0), u);
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

fn fbm2d(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i = 0; i < 3; i++) {
        v += a * noise2d(pos);
        pos = pos * 2.04 + vec2<f32>(12.3, 34.5);
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let across = uv.x;
    let along = uv.y;
    let r2 = dot(uv, uv);
    let r = sqrt(r2);

    let team_hdr = fx.primary_color.rgb;
    let white_hot = vec3<f32>(1.0, 0.98, 0.92);

    var final_color = vec3<f32>(0.0);
    var final_alpha = 0.0;

    // -------------------------------------------------------------
    // MODE 0: HIGH-ENERGY CONTINUOUS LASER BEAM
    // -------------------------------------------------------------
    if (fx.fx_kind == 0u) {
        let core = exp(-across * across * 110.0) * 3.5;
        let corona_inner = exp(-across * across * 14.0) * 1.6;
        let corona_turb = exp(-across * across * 3.5) * 0.5 * fbm2d(vec2<f32>(along * 6.0, globals.time * 8.0));
        let corona = corona_inner + corona_turb;

        let pulse = sin(along * fx.aspect * 4.0 - globals.time * 45.0) * 0.25 + 0.75;

        let d_muzzle = length(vec2<f32>(across, along + 0.90));
        let d_impact = length(vec2<f32>(across, along - 0.90));
        let muzzle_flare = exp(-d_muzzle * d_muzzle * 35.0) * 4.0;
        let impact_flare = exp(-d_impact * d_impact * 28.0) * 5.0;

        let life_fade = smoothstep(1.0, 0.75, fx.age) * smoothstep(0.0, 0.05, fx.age);

        final_color = (white_hot * (core + muzzle_flare + impact_flare) + team_hdr * corona) * pulse;
        final_alpha = (core * 0.9 + corona * 0.7 + muzzle_flare + impact_flare) * life_fade;
    }

    // -------------------------------------------------------------
    // MODE 1: SUPERSONIC HITSCAN / RAILGUN TRACER BOLT
    // -------------------------------------------------------------
    else if (fx.fx_kind == 1u) {
        let bolt_head = mix(-0.95, 0.95, smoothstep(0.0, 0.35, fx.age));
        let bolt_tail = mix(-0.95, 0.95, smoothstep(0.20, 0.95, fx.age));

        let in_bolt = smoothstep(bolt_tail - 0.08, bolt_tail, along) * smoothstep(bolt_head + 0.04, bolt_head, along);
        let core = exp(-across * across * 85.0) * in_bolt * 4.0;
        let mach_rings = pow(max(0.0, cos((along - bolt_head) * 25.0)), 4.0) * exp(-across * across * 25.0) * in_bolt;

        let impact_burst = exp(-r2 * 24.0) * smoothstep(0.35, 0.95, fx.age) * (1.0 - fx.age) * 6.0;

        final_color = white_hot * (core + impact_burst) + team_hdr * (mach_rings * 2.0);
        final_alpha = (core * 0.85 + mach_rings * 0.5 + impact_burst) * (1.0 - fx.age);
    }

    // -------------------------------------------------------------
    // MODE 2: 3-PHASE BRAIDED LIGHTNING ARC
    // -------------------------------------------------------------
    else if (fx.fx_kind == 2u) {
        let t = globals.time * 30.0;
        let d1 = (noise1d(along * 8.0 + t + fx.seed * 11.0) - 0.5) * 0.55;
        let d2 = (noise1d(along * 14.0 - t * 1.3 + fx.seed * 23.0) - 0.5) * 0.40;
        let d3 = (noise1d(along * 22.0 + t * 0.7 + fx.seed * 37.0) - 0.5) * 0.25;

        let bolt1 = exp(-pow((across - d1) * 16.0, 2.0)) * 2.5;
        let bolt2 = exp(-pow((across - d2) * 18.0, 2.0)) * 1.8;
        let bolt3 = exp(-pow((across - d3) * 22.0, 2.0)) * 1.5;

        let strobe = step(0.15, hash11(floor(t * 1.5) + fx.seed));
        let total_lightning = (bolt1 + bolt2 + bolt3) * strobe;

        let life = 1.0 - fx.age;
        final_color = mix(team_hdr, white_hot, total_lightning * 0.35);
        final_alpha = total_lightning * life * 0.9;
    }

    // -------------------------------------------------------------
    // MODE 3: SWEEPING CRESCENT PSI-BLADE MELEE SLASH
    // -------------------------------------------------------------
    else if (fx.fx_kind == 3u) {
        if (r2 > 1.0) {
            discard;
        }

        let slash_angle = fx.primary_color.w;
        let theta = atan2(uv.y, uv.x) - slash_angle;
        let normalized_theta = (theta + 3.14159265) % 6.2831853 - 3.14159265;

        let crescent_mask = smoothstep(1.35, 0.70, abs(normalized_theta));
        if (crescent_mask <= 0.001) {
            discard;
        }

        let blade_edge = exp(-pow((r - 0.72) * 18.0, 2.0)) * 4.5;
        let trail_turb = fbm2d(vec2<f32>(r * 8.0, normalized_theta * 4.0 - fx.age * 12.0));
        let trailing_veil = smoothstep(0.35, 0.70, r) * trail_turb * 1.8;

        let life_fade = (1.0 - fx.age) * smoothstep(0.0, 0.05, fx.age);
        let intensity = (blade_edge + trailing_veil) * crescent_mask * life_fade;

        final_color = mix(team_hdr, white_hot, blade_edge * 0.5);
        final_alpha = intensity * 0.85;
    }

    // -------------------------------------------------------------
    // MODE 4: BLAZING COMET-TAIL PLASMA PROJECTILE ORB
    // -------------------------------------------------------------
    else if (fx.fx_kind == 4u) {
        let head_dist = length(uv - vec2<f32>(0.0, 0.30));
        let core = exp(-head_dist * head_dist * 45.0) * 5.0;
        let halo = exp(-head_dist * head_dist * 8.0) * 1.5;

        let tail_along = smoothstep(-0.95, 0.30, along);
        let tail_across = exp(-across * across * (18.0 - along * 8.0));
        let tail_turb = fbm2d(vec2<f32>(across * 4.0, along * 6.0 - globals.time * 12.0));
        let tail = tail_along * tail_across * tail_turb * 1.4;

        final_color = white_hot * core + team_hdr * (halo + tail);
        final_alpha = clamp((core + halo * 0.7 + tail * 0.8) * (1.0 - fx.age * 0.5), 0.0, 1.0);
    }

    // -------------------------------------------------------------
    // MODE 5: 8-POINTED IMPACT STARBURST & REFRACTIVE SHOCKWAVE
    // -------------------------------------------------------------
    else if (fx.fx_kind == 5u) {
        if (r2 > 1.0) {
            discard;
        }

        let theta = atan2(uv.y, uv.x);
        let ray = pow(abs(cos(4.0 * theta)), 8.0) * exp(-r * 3.5) * 3.5;
        let core_burst = exp(-r2 * 36.0) * 6.0;

        let wave_r = smoothstep(0.0, 0.95, fx.age) * 0.85;
        let ring = exp(-pow((r - wave_r) * 18.0, 2.0)) * 2.2;

        let life = (1.0 - fx.age) * (1.0 - fx.age);
        final_color = white_hot * (core_burst + ring * 0.5) + team_hdr * (ray + ring);
        final_alpha = (core_burst * 0.9 + ray * 0.6 + ring * 0.7) * life;
    }

    if (final_alpha < 0.008) {
        discard;
    }

    return vec4<f32>(final_color * final_alpha, final_alpha);
}
```

---

## 6. Rust Implementation Specifications

### 6.1 `crates/sim_core/src/steering.rs`
1. Introduce `const PUSH_RELAXATION: SimScalar = SimScalar::from_f32_lossy(0.35);`.
2. In `compute_push_displacement`:
   - Scale `overlap = ((combined_radius - distance) * PUSH_RELAXATION).max(SimScalar::ZERO)`.
   - Update Rule 4 (moving vs moving): reduce tangential component from $0.75$ to $0.25$.
3. In `compute_unified_local_steering`:
   - Increase deadzone threshold to $0.05$ with deterministic tie-breaker `unit_id.0 < blocker_id.0`.

---

### 6.2 `crates/sim_core/src/world.rs`
1. Record each entity's start-of-tick position before movement: `entity.pos_at_tick_start = entity.position;`.
2. In `resolve_collisions_and_push`:
   - Identify `is_actively_engaged` units and treat them as unpushable by friendly units.
   - Accumulate `net_pushes: BTreeMap<EntityId, SimVec2>`.
   - Clamp each entity's `|net_push| <= max_speed * 0.65`.
   - Clamp total tick displacement: `|(entity.position + net_push) - entity.pos_at_tick_start| <= max_speed`.

---

### 6.3 `crates/client/src/vfx.rs`
1. Define `CombatFxUniformGpu` (32 bytes, Pod + Zeroable) and `CombatFxMaterial` (`AlphaMode::Add`, `depth_bias: -25.0`).
2. Implement `emit_mist_wake`:
   - Stationary sweating: uniform random spherical spawn, zero horizontal push, gentle upward drift.
   - Live battle movement: zero backward ejection, inward centerline suction + forward entrainment, suction scale contraction $1.90R \to 0.85R$.
   - Visual Lab: equatorial spawn with relative suction tail tapering $1.95R \to 0.85R$.
3. Overhaul `process_combat_events_system` to dispatch all combat visuals to `combat_fx.wgsl`.

---

### 6.4 `crates/client/src/scene_controller.rs`
1. Scale live battle tick displacement to world speed per second: `world_vel = instance.velocity * 25.0`.
2. Implement frame-rate independent transform lerping between simulation ticks:
   `transform.translation = transform.translation.lerp(pos, (time.delta_secs() * 22.0).min(1.0));`.

---

## 7. Verification & Testing Strategy

### 7.1 Automated Convergence Smoothness & Speed-Cap Test
In `crates/sim_core/tests/collision_and_immobility_tests.rs`:
Add `test_converging_units_respect_speed_cap_and_do_not_vibrate`:
- Spawn 1 enemy target at $(20, 20)$ and 8 friendly melee units surrounding it within a tight radius ($r \approx 2.0 \dots 4.0$).
- Order all units to `AttackTarget`.
- Step the simulation for 120 ticks.
- **Assertion 1 (Strict Speed Cap):** For every unit on every tick:
  $$\|\vec{P}(t) - \vec{P}(t-1)\| \le \text{max\_speed} + 0.002$$
  Zero tolerance for push-velocity overshoot!
- **Assertion 2 (Anti-Vibration):** Check for ping-pong directional reversals:
  A reversal is defined as:
  $$\vec{\Delta}(t) \cdot \vec{\Delta}(t-1) < -0.3 \quad \text{and} \quad \|\vec{\Delta}\| > 0.015$$
  The count of reversals per unit across all 120 ticks must be $\le 1$ (units settle into a smooth concave arc without bouncing).

### 7.2 Combat VFX Shader Tests
In `crates/client/tests/round2_visual_tests.rs`:
1. `test_combat_fx_uniform_and_shader_invariants`:
   - `size_of::<CombatFxUniformGpu>() == 32`.
   - `align_of::<CombatFxUniformGpu>() == 16`.
   - Validates existence of `combat_fx.wgsl` and all 6 procedural modes.
2. `test_combat_fx_material_alpha_and_depth_bias`:
   - Validates `AlphaMode::Add` and `depth_bias: -25.0`.
3. `test_all_combat_vfx_entities_despawn_after_lifetime`:
   - Validates $100\%$ despawn after timers complete across all 6 effects.

---

## 8. Implementation Roadmap for Stage 2 Sub-Agents

1. **Phase 1: Anti-Vibration & Speed-Cap Physics (`sim_core`)**
   - Update `crates/sim_core/src/steering.rs` with `PUSH_RELAXATION = 0.35` and deadzone hysteresis.
   - Update `crates/sim_core/src/world.rs` with net push accumulation, engagement anchoring, and speed clamping.
   - Add convergence smoothness integration test in `crates/sim_core/tests/collision_and_immobility_tests.rs`.
2. **Phase 2: Procedural Combat Shader (`combat_fx.wgsl`)**
   - Write `assets/shaders/combat_fx.wgsl` with all 6 procedural modes.
3. **Phase 3: Client VFX & Sweating Mist (`vfx.rs`)**
   - Refactor `emit_mist_wake` for surface sweating and aerodynamic suction wake.
   - Refactor `process_combat_events_system` to spawn volumetric ribbon/quad meshes.
4. **Phase 4: Client Scene Controller & Render Smoothing (`scene_controller.rs` & `lib.rs`)**
   - Implement inter-tick transform smoothing in `sync_live_battle_system`.
   - Register `MaterialPlugin::<CombatFxMaterial>` and systems in `crates/client/src/lib.rs`.
5. **Phase 5: Quality Gate Verification**
   - Execute `cargo test --workspace`.
   - Execute `cargo clippy --workspace --all-targets -- -D warnings`.
   - Execute `cargo fmt --all -- --check`.
