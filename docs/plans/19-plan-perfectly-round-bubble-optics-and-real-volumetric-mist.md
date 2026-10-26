# Plan 19: Perfectly Round Bubble Optics, Perspective Anamorphosis Correction, and Real Volumetric Curl-Warped Mist

**Document Status:** Architectural Blueprint (Stage 1)  
**Author:** Principal Graphics & Volumetric Shader Architect (`pro` model)  
**Target Crates:** [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl), [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl), [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs), [`crates/client/src/scene_controller.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs)  
**Binding Standards:** [`AGENTS.md`](file:///home/cl/projects/space-scuffle/AGENTS.md) (Textbook-Grade Engineering, Zero Placeholders, Strict Invariants)  
**Visual Artifacts Under Diagnosis:**
- [User Feedback Screenshot `media_1793022250145_2edb635a.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793022250145_2edb635a.png)
- [Target Visual Benchmark `Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg)

---

## 1. Executive Summary & Forensic Diagnosis of User Feedback

### 1.1 The User's Feedback
> *"the mist looks now much more natural, but it's still only a start. Please investigate how to make real mist.*  
> *I have also noticed, that the bubbles don't always look round (see screenshot)."*

A pixel-level and mathematical forensic analysis of the user screenshot ([`media_1793022250145_2edb635a.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793022250145_2edb635a.png)) reveals that every single defect reported is rooted in specific mathematical formulations inside [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl), [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl), and [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs).

```
+---------------------------------------------------------------------------------------------------+
| FORENSIC BREAKDOWN OF VISUAL DEFECTS IN media_1793022250145_2edb635a.png                         |
+---------------------------------------------------------------------------------------------------+
| 1. BUBBLE NON-ROUNDNESS:                                                                          |
|    - Bright White Arch across forehead (bubble_dome.wgsl:160-178):                                |
|      `meridian_stripe` projects as an interior elliptical arc (11 o'clock to 3 o'clock) at HDR   |
|      intensity 5.32. The human eye parses this white arc as the top-right edge, while the real    |
|      glass boundary bulges out above-right, creating an egg/lemon shape with a bursting head!     |
|    - Perspective Anamorphosis (bubble_dome.wgsl:81-88):                                           |
|      Normal projected via per-fragment ray basis skews the disk off-center under perspective.     |
|    - Specular Blob on Chest (bubble_dome.wgsl:149-158):                                           |
|      Broad specular lobe at r ~ 0.25 places a giant white spot directly over the unit's torso.    |
|                                                                                                   |
| 2. FAKE "STRIPED/DOTTED MUSHROOM" MIST:                                                           |
|    - Stationary `seed_val` Bug (vfx.rs:389-404):                                                  |
|      When speed <= 0.0005, `seed_val` depended ONLY on bubble_pos without time/frame variation.   |
|      Every stationary bubble spawned particles at the exact same two upper-left angles (10 & 11h)|
|      creating two identical frozen bumps behind every bubble!                                     |
|    - Artificial Parallel Stripes & Circle Grid (mist_wake.wgsl:96-117):                           |
|      `across_trail * 12.0` drew 6 horizontal parallel stripes across each puff. `b_grid` stamped  |
|      a Cartesian grid of tiny circles. Discs had hard circular button outlines (1 - r^2).         |
|    - Muddy Brown Additive Squaring (mist_wake.wgsl:159-165):                                      |
|      `final_color * alpha` with alpha = 0.45 under AlphaMode::Add produced 0.20 brightness!       |
+---------------------------------------------------------------------------------------------------+
```

---

## 2. Root Cause Analysis: Bubble Non-Roundness & Optical Corrections

### 2.1 Culprit 1A: The Meridian Facing Stripe Destroys the Sphere Silhouette
In [`assets/shaders/bubble_dome.wgsl:160-178`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl#L160-L178):
```wgsl
let horiz_normal = normalize(vec3<f32>(normal.x, 0.0001, normal.z));
let forward_align = dot(horiz_normal, heading_dir);
let heading_right = vec3<f32>(-heading_dir.z, 0.0, heading_dir.x);
let lateral_dist = abs(dot(normal, heading_right));
let meridian_stripe = smoothstep(0.08, 0.02, lateral_dist) * smoothstep(0.15, 0.95, forward_align);
let eq_band = smoothstep(0.045, 0.010, abs(normal.y + 0.18)) * smoothstep(0.10, 0.85, forward_align);
...
direction_stripe = mix(bubble.team_color.rgb, vec3<f32>(1.0, 1.0, 1.0), 0.35) * stripe_total * 2.8;
```

#### The Geometric Flaw
When a unit faces right (`+X`, $\theta = 0$), `heading_dir = (1, 0, 0)` and `heading_right = (0, 0, 1)`. The condition `lateral_dist == 0` defines the 3D great circle on the sphere where $normal.z = 0$.
The game camera views the scene from an isometric RTS elevation pitched down at $\approx 58^\circ$ from vertical:
$$\vec{V}_{\text{cam}} \approx (0, 0.85, 0.53)$$
Under this viewing angle, the 3D meridian circle $normal.z = 0, normal.x > 0$ projects onto the 2D view screen as an **inclined elliptical arch crossing the character's forehead from 11 o'clock down to 3 o'clock** inside the disk radius ($r \approx 0.35 \dots 0.85$).

Because `direction_stripe` has HDR intensity $2.8 \times 1.9 = 5.32$ (exceeding the outer Fresnel rim), the human visual system's edge-detection mechanics perceive this bright white line as the *top-right outer boundary* of the object. However, the true sphere mesh continues behind and above that line ($normal.z < 0, normal.y > 0.5$). The character's head and the upper-right glass bulge out beyond the white line, producing a deformed, lopsided egg or lemon shape.

#### Architectural Solution
**Never draw lines across the interior of the sphere ($r < 0.82$).**
Facing direction must be indicated via a **sleek directional rim accent confined strictly within the outer circular Fresnel annulus ($r \in [0.86, 0.98]$)**:
1. Project the 3D heading vector `heading_dir` onto the camera's view-plane basis `(cam_right, cam_up)`:
   $$\vec{H}_{2D} = \text{normalize}\left(\vec{H} \cdot \vec{R}_{\text{cam}}, \vec{H} \cdot \vec{U}_{\text{cam}}\right)$$
2. Measure the angular alignment of the current fragment's screen-projected normal:
   $$\vec{D}_{\text{dir}} = \frac{\vec{D}}{\max(r, 0.001)}, \quad \text{align} = \vec{D}_{\text{dir}} \cdot \vec{H}_{2D}$$
3. Render a luminous forward nose accent **strictly inside the circular rim**:
   $$\text{facing\_accent} = \text{smoothstep}(0.65, 0.96, \text{align}) \times \text{smoothstep}(0.86, 0.93, r) \times (1.0 - \text{smoothstep}(0.96, 0.99, r))$$
Because this accent exists *entirely within* the outer circular annulus, it reinforces the perception of a razor-sharp, perfectly round circle from any camera angle while unambiguously indicating heading direction!

---

### 2.2 Culprit 1B: Perspective Anamorphosis & Camera Basis vs. Normal Silhouette
In [`assets/shaders/bubble_dome.wgsl:81-88`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl#L81-L88):
```wgsl
let cam_forward = normalize(in.world_position.xyz - view.world_position);
let cam_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), cam_forward));
let cam_up = cross(cam_forward, cam_right);
let disk = vec2<f32>(dot(normal, cam_right), -dot(normal, cam_up));
let r2 = dot(disk, disk);
let r = sqrt(min(r2, 1.0));
```

#### The Geometric Flaw
1. `cam_forward` was computed on a per-fragment basis from `in.world_position.xyz - view.world_position`. On off-center entities, view rays diverge from the camera's true view plane, causing perspective distortion (anamorphosis) where the disk coordinate system is skewed.
2. In perspective projection, the apparent outer edge of a sphere is the locus of points where the line of sight from the camera eye is tangent to the sphere surface:
   $$\vec{N} \cdot \vec{V} = 0, \quad \text{where } \vec{V} = \text{normalize}(\vec{P}_{\text{cam}} - \vec{P}_{\text{surface}})$$
Computing $r$ from screen-projected normal coordinates `dot(normal, cam_right)^2 + dot(normal, cam_up)^2` does not equal $1.0$ at the true perspective silhouette horizon when the sphere is off-axis.

#### Architectural Solution
Use the camera's true view-plane basis directly from Bevy's `view.world_from_view` matrix:
```wgsl
let cam_right = normalize(view.world_from_view[0].xyz);
let cam_up = normalize(view.world_from_view[1].xyz);
let cam_forward = -normalize(view.world_from_view[2].xyz);
```
Define the optical radius $r$ using the exact physical tangency relation:
$$NdotV = \max(\vec{N} \cdot \vec{V}, 0.0)$$
$$r = \sqrt{\max(1.0 - NdotV \times NdotV, 0.0)}$$
- At the center of the sphere facing the camera: $\vec{N} = \vec{V} \implies NdotV = 1.0 \implies r = 0.0$.
- At the physical silhouette boundary: $\vec{N} \perp \vec{V} \implies NdotV = 0.0 \implies r = 1.0$.
This guarantees that $r = 1.0$ matches the $360^\circ$ silhouette edge of the 3D sphere mesh from **any camera position and any FOV**, eliminating perspective ovalization.

The 2D disk coordinates for lens sampling and directional indicators are obtained by projecting the normal onto the camera plane and scaling by $r$:
```wgsl
let proj_norm = vec2<f32>(dot(normal, cam_right), dot(normal, cam_up));
let disk_dir = normalize(proj_norm);
let disk = disk_dir * r;
```

---

### 2.3 Culprit 1C: Specular Glare Blob Obscuring the Unit's Chest
In [`assets/shaders/bubble_dome.wgsl:149-158`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl#L149-L158):
```wgsl
let l1 = normalize(vec3<f32>(-0.6, 0.8, 0.4));
let L1 = normalize(view_dir + l1);
let glare1 = pow(max(dot(normal, L1), 0.0), 48.0) * 1.35;
```
Because the light direction $l_1 = (-0.6, 0.8, 0.4)$ and view direction $\vec{V} \approx (0, 0.85, 0.53)$ are nearly aligned, the half-vector $\vec{L}_1$ points almost directly toward the front pole of the sphere ($r \approx 0.25$). This paints an opaque, $1.35 \times$ HDR white spotlight over the character's chest in all three units in the screenshot.

#### Architectural Solution: Studio Crescent Rim Highlights
In physical glass and crystal spheres ([`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg)), studio reflections appear as sleek **crescents near the curved glass rim**, while the central aperture ($r < 0.65$) remains 100% crystal clear:
1. **Upper-Left Key Crescent:** High overhead key light angled from top-left, modulated by outer rim mask $\text{smoothstep}(0.68, 0.88, r) \times (1.0 - \text{smoothstep}(0.94, 0.99, r))$.
2. **Lower-Right Soft Bounce:** Subtle ground reflection near $r \in [0.78, 0.94]$.
3. **Central Clearance Guarantee:** Modulate all specular highlights by $\text{smoothstep}(0.65, 0.85, r)$ so that for $r < 0.65$, specular is mathematically $0.0$, guaranteeing an unobstructed view of the character video.

---

## 3. Root Cause Analysis: Fake Mist vs. Real Volumetric Mist

### 3.1 Culprit 2A: The Stationary `seed_val` Freezing Bug
In [`crates/client/src/vfx.rs:389-404`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs#L389-L404):
```rust
let seed_val = ((bubble_pos.x * 13.7 + bubble_pos.z * 37.1) * 10.0).fract().abs();
for i in 0..2 {
    let phi = (seed_val * std::f32::consts::TAU + (i as f32) * std::f32::consts::PI).cos() * std::f32::consts::PI;
    let theta = (seed_val * 3.7 + (i as f32) * 2.1).sin() * std::f32::consts::PI;
    let radial_dir = Vec3::new(theta.sin() * phi.cos(), phi.sin().abs() * 0.75 + 0.25, theta.sin() * phi.sin()).normalize_or_zero();
    let spawn_pos = bubble_pos + radial_dir * (radius * 0.72) + Vec3::new(0.0, radius * 0.10, 0.0);
    ...
```

#### The Bug
When a unit is stationary ($speed \le 0.0005$), `bubble_pos` is constant. Therefore `seed_val`, `phi`, `theta`, and `spawn_pos` are **identical constants on every single frame**!
In the screenshot [`media_1793022250145_2edb635a.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793022250145_2edb635a.png), all three bubbles display the exact same two striped lumps sticking out of the upper-left of each sphere at 10 o'clock and 11 o'clock.

#### Architectural Solution
1. Introduce a monotonically advancing emission seed salt using an atomic counter (`static EMIT_COUNTER: AtomicU32`) combined with the golden ratio hash:
   $$\text{seed\_salt} = \text{fract}(\text{emit\_id} \times 0.6180339887)$$
2. Distribute stationary perspiration uniformly across all $360^\circ$ ($\theta \in [0, 2\pi)$) around the bubble's footprint.
3. Spawn soft, large, low-opacity vapor veils ($scale = 2.2R \dots 2.8R$) that gently rise and drift outward, simulating cold dry-ice fog sublimating off the glass.

---

### 3.2 Culprit 2B: Artificial Parallel Stripes & Circle Grid
In [`assets/shaders/mist_wake.wgsl:96-117`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl#L96-L117):
```wgsl
let streak_coord = vec2<f32>(along_trail * 1.5 - globals.time * 3.0, across_trail * 12.0);
let streaks = pow(noise2d(streak_coord), 2.2) * smoothstep(0.1, 0.8, taper);
...
let b_grid = fract(vec2<f32>(along_trail * 3.0, across_trail * 4.0) + ...) - 0.5;
```
1. `across_trail * 12.0`: When stationary, `flow_angle = 0.0`, so `across_trail` corresponds to the vertical screen axis. Scaling by $12.0$ generates 6 distinct horizontal parallel stripes across every stationary puff!
2. `b_grid`: Generates a rigid Cartesian lattice of circular soap bubbles.
3. Envelope: `tail_envelope = ... * (1.0 - r2)` and `condensation_core = exp(-r2 * 6.0)` impose hard circular disc contours.
4. Color attenuation: `return vec4<f32>(final_color * alpha, alpha)` with `alpha = clamp(density * 0.45, 0.0, 0.70)` under `AlphaMode::Add` square-attenuates brightness ($0.45 \times 0.45 = 0.20$), turning additive mist dark brown/gray against dark terrain.

---

## 4. Architectural Design: Real Volumetric Curl-Warped Mist Pipeline

To produce the ethereal, silky, multicolored mist wakes seen in the benchmark artwork ([`Gemini_Generated_Image_12da9g12da9g12da.jpg`](file:///home/cl/projects/space-scuffle/tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg)), we specify a textbook-grade **Curl-Warped Volumetric Mist Engine**:

```
+---------------------------------------------------------------------------------------------------+
| REAL VOLUMETRIC MIST ENGINE ARCHITECTURE                                                          |
+---------------------------------------------------------------------------------------------------+
|                                                                                                   |
|  1. COORDINATE DOMAIN WARPING (assets/shaders/mist_wake.wgsl)                                     |
|     - Octave Rotations: mat2x2<f32>(0.80, -0.60, 0.60, 0.80) rotates octaves by 36.87 degrees    |
|       ==> Zero Cartesian grid alignment! Zero horizontal/vertical stripes!                        |
|     - 2-Stage Domain Warping:                                                                     |
|       q = (fbm(p + drift_1), fbm(p + drift_2))                                                    |
|       r_warp = (fbm(p + 2.4*q + drift_3), fbm(p + 2.4*q + drift_4))                              |
|       turb = fbm(p + 2.4*r_warp)                                                                  |
|                                                                                                   |
|  2. ORGANIC NOISE EROSION (Eliminates Circular Disc Outlines)                                     |
|     - Base radial distance: base_falloff = clamp(1.0 - r2, 0.0, 1.0)                              |
|     - Cloud erosion: cloud_density = smoothstep(0.12, 0.85, turb * pow(base_falloff, 0.65))       |
|       ==> Feathered, wispy, tendril-like vapor shapes with ZERO circular disc outlines!           |
|                                                                                                   |
|  3. CONTINUOUS MULTI-LAYER KINEMATICS (crates/client/src/vfx.rs)                                  |
|     - Stationary Perspiration ("Sweating Orbs"):                                                  |
|       * Monotonically advancing seed salt: seed_salt = fract(emit_counter * 0.6180339887)         |
|       * 360-degree radial coverage around sphere base                                             |
|       * Large, low-opacity vapor veils: scale 2.2R -> 2.8R, thermal rise (0, 0.08, 0)             |
|       * Depth bias -45.0: crisp crystal bubble always renders in front!                           |
|     - Moving Aerodynamic Air-Suction Slipstream:                                                  |
|       * Continuous emission every frame (% 1 == 0 when moving) ==> No stepped gaps!               |
|       * Low-pressure suction pocket pulls mist inward: v_inward = -lateral / 0.28                 |
|       * Forward slipstream velocity: v_forward = forward_dir * (speed * 0.18)                     |
|       * Aerodynamic necking: scale contracts 2.0R -> 1.1R in wake pocket                          |
|       * Exponential air drag: v *= exp(-3.5 * dt) brings mist to dead halt in world space!        |
|                                                                                                   |
|  4. LUMINOUS BIOLUMINESCENT COLOR COMPOSITION (Benchmark Match)                                   |
|     - Flow Core: Electric Cyan vec3(0.12, 0.88, 1.0)                                             |
|     - Intermediate Turbulence: Neon Violet-Magenta vec3(0.85, 0.22, 1.0)                         |
|     - High-Density Filaments: Pearlescent Ice-White vec3(0.92, 0.96, 1.0)                         |
|     - Outer Fringes & Sparks: Fiery Golden-Amber vec3(1.0, 0.58, 0.14)                            |
|     - Additive Energy: vec4(hdr_color * alpha * 2.2, alpha) under AlphaMode::Add                  |
|       ==> Self-illuminating, vibrant, ethereal mist with zero muddy brown/gray pixels!            |
+---------------------------------------------------------------------------------------------------+
```

---

## 5. Exact WGSL & Rust Code Specifications

### 5.1 Bubble Dome Shader Specification: [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl)

The complete revised fragment shader for [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl):

```wgsl
// assets/shaders/bubble_dome.wgsl
// Unified shader for Mobile Unit Spheres and Building Hemisphere Domes in Space Scuffle.
// Implements Textbook-Grade Spherical Glass Optics:
// - Perspective-correct silhouette horizon: r = sqrt(1.0 - NdotV^2) matches 360-degree boundary from ANY camera angle!
// - View-aligned camera plane basis from view.world_from_view matrix (zero perspective anamorphosis)
// - Studio crescent rim specular highlights (upper-left key & lower-right bounce) leaving center 100% crystal clear
// - Directional indicator re-engineered as a sleek rim arc confined strictly to outer Fresnel annulus [0.86, 0.98]
// - Dynamic video texture array sampling with spherical lens magnification
// - Voronoi glass cracks & CRT scanlines

#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::forward_io::VertexOutput

struct BubbleUniform {
    team_color: vec4<f32>,
    world_pos: vec3<f32>,
    radius: f32,
    shape_kind: u32,             // 0 = UnitSphere, 1 = BuildingDome
    heading_angle_rad: f32,
    aim_angle_rad: f32,
    is_attacking: u32,
    anim_state: u32,
    anim_progress: f32,          // Normalized progress (0.0..1.0)
    hp_ratio: f32,               // 1.0 = pristine, <0.75 = cracks, <0.35 = critical
    shield_ratio: f32,           // 0.0..1.0 outer sheen
    shield_hit_flash: f32,       // Decaying ripple intensity
    hull_hit_angle_rad: f32,     // Direction of last damage impact
    hull_hit_flash: f32,         // Decaying impact flare
    build_progress: f32,         // Vertical construction scanline fill (0.0..1.0)
    selected: u32,
    cloaked: u32,
    feature_flags: u32,          // Bitmask controlling modular optics (offset 88)
    frame_index: u32,            // Exact active frame in texture array (offset 92)
};

@group(2) @binding(0) var<uniform> bubble: BubbleUniform;
@group(2) @binding(1) var video_texture: texture_2d_array<f32>;
@group(2) @binding(2) var video_sampler: sampler;

// Modular Feature Bitflags
const FLAG_CLOAKED: u32 = 1u;         // 1 << 0
const FLAG_SELECTED: u32 = 2u;        // 1 << 1
const FLAG_GLARE: u32 = 4u;           // 1 << 2: Studio crescent rim specular reflections
const FLAG_FRESNEL: u32 = 8u;         // 1 << 3: Iridescent thin-film Fresnel rim
const FLAG_INNER_WARP: u32 = 16u;     // 1 << 4: View-aligned spherical lens warp
const FLAG_CRACKS: u32 = 32u;         // 1 << 5: Voronoi glass cracks & electrical arcing
const FLAG_CRT_GLITCH: u32 = 64u;     // 1 << 6: CRT scanlines & chromatic aberration
const FLAG_SPEED_BOOSTED: u32 = 128u; // 1 << 7: Hyper wake glow
const FLAG_FACING_STRIPE: u32 = 256u; // 1 << 8: Rim-confined facing indicator
const FLAG_ATTACK_APERTURE: u32 = 512u;// 1 << 9: Target-lock weapon aperture

// Procedural 2D hash for Voronoi glass cracks
fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

// Computes Voronoi fracture distance
fn voronoi_cracks(uv: vec2<f32>) -> f32 {
    let g = floor(uv * 8.0);
    let f = fract(uv * 8.0);
    var min_dist = 1.0;

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let point = hash2(g + neighbor);
            let diff = neighbor + point - f;
            min_dist = min(min_dist, length(diff));
        }
    }
    return smoothstep(0.04, 0.01, min_dist);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let view_dir = normalize(view.world_position - in.world_position.xyz);
    let normal = normalize(in.world_normal);

    // 1. Perspective-Correct Silhouette Horizon & Camera Basis
    // Exact physical tangency: N · V = 0 defines the apparent horizon of any sphere under perspective!
    let NdotV = max(dot(normal, view_dir), 0.0);
    let r2 = max(1.0 - NdotV * NdotV, 0.0);
    let r = sqrt(r2); // Exactly 0.0 at center, exactly 1.0 at physical silhouette boundary!

    // Camera view-plane basis directly from Bevy view matrix
    let cam_right = normalize(view.world_from_view[0].xyz);
    let cam_up = normalize(view.world_from_view[1].xyz);
    let cam_forward = -normalize(view.world_from_view[2].xyz);

    // Project normal onto camera plane to obtain screen-space direction vector
    let proj_norm = vec2<f32>(dot(normal, cam_right), dot(normal, cam_up));
    let disk_dir = normalize(proj_norm);
    let disk = disk_dir * r;

    // Optical glass lens refraction and magnification
    let z_lens = sqrt(max(1.0 - r2, 0.0));
    var lens_uv = vec2<f32>(0.5, 0.5) + disk * mix(0.48, 0.35, z_lens);

    // Facing-aware horizontal mirroring
    let heading_dir = vec3<f32>(cos(bubble.heading_angle_rad), 0.0, sin(bubble.heading_angle_rad));
    if (dot(heading_dir, cam_right) < 0.0) {
        lens_uv.x = 1.0 - lens_uv.x;
    }

    // Lens aperture vignette & edge depth darkening (kicks in only near glass rim r > 0.85)
    let lens_mask = 1.0 - smoothstep(0.88, 0.99, r);
    let glass_depth_tint = 1.0 - smoothstep(0.80, 0.99, r) * 0.40;

    // CRT Scanlines & Glitch Aberration
    var scanline_mult = 1.0;
    var uv_offset = vec2<f32>(0.0);
    if ((bubble.feature_flags & FLAG_CRT_GLITCH) != 0u) {
        if (bubble.hp_ratio < 0.50) {
            scanline_mult = 0.85 + 0.15 * sin(lens_uv.y * 240.0);
        }
        if (bubble.hp_ratio < 0.35) {
            let glitch_jitter = sin(globals.time * 40.0) * 0.015;
            uv_offset = vec2<f32>(0.02 + glitch_jitter, 0.0);
        }
    }

    // Texture Array Sampling
    let layer_index = i32(bubble.frame_index);
    let sample_r = textureSample(video_texture, video_sampler, lens_uv + uv_offset, layer_index).r;
    let sample_g = textureSample(video_texture, video_sampler, lens_uv, layer_index).g;
    let sample_b = textureSample(video_texture, video_sampler, lens_uv - uv_offset, layer_index).b;
    var video_color = pow(vec3<f32>(sample_r, sample_g, sample_b), vec3<f32>(0.9)) * 1.45 * scanline_mult * lens_mask * glass_depth_tint;

    // Progressive Construction Scanline Fill
    if (bubble.anim_state == 0u) { // Constructing
        if (in.uv.y > bubble.build_progress) {
            let grid_line = step(0.92, fract(in.uv.y * 20.0)) + step(0.92, fract(in.uv.x * 20.0));
            video_color = vec3<f32>(0.1, 0.6, 1.0) * grid_line * 2.0;
        } else if (abs(in.uv.y - bubble.build_progress) < 0.02) {
            video_color += vec3<f32>(1.0, 0.9, 0.4) * 3.0;
        }
    }

    // 2. Crystal-Clear Center with Steep Iridescent Fresnel Rim
    let F = pow(r, 4.0);
    var rim_color = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_FRESNEL) != 0u) {
        let iridescence = vec3<f32>(
            0.5 + 0.5 * cos(6.28318 * (F + 0.0)),
            0.5 + 0.5 * cos(6.28318 * (F + 0.333)),
            0.5 + 0.5 * cos(6.28318 * (F + 0.667))
        );
        rim_color = mix(bubble.team_color.rgb, iridescence, 0.40) * F * 3.2;
    }

    // 3. Studio Crescent Rim Specular Highlights (Center r < 0.65 is 100% CLEAR!)
    var specular = 0.0;
    if ((bubble.feature_flags & FLAG_GLARE) != 0u) {
        // Upper-left key light crescent
        let l1 = normalize(cam_up * 0.70 - cam_right * 0.55 + cam_forward * 0.45);
        let H1 = normalize(view_dir + l1);
        let spec1 = pow(max(dot(normal, H1), 0.0), 36.0);
        let rim_mask1 = smoothstep(0.68, 0.88, r) * (1.0 - smoothstep(0.95, 1.0, r));
        let glare1 = spec1 * rim_mask1 * 2.4;

        // Lower-right ambient bounce crescent
        let l2 = normalize(-cam_up * 0.60 + cam_right * 0.50 + cam_forward * 0.35);
        let H2 = normalize(view_dir + l2);
        let spec2 = pow(max(dot(normal, H2), 0.0), 20.0);
        let rim_mask2 = smoothstep(0.78, 0.92, r) * (1.0 - smoothstep(0.97, 1.0, r));
        let glare2 = spec2 * rim_mask2 * 0.85;

        specular = glare1 + glare2;
    }

    // 4. Directional Indicator: Sleek Rim Arc Confined Strictly to Outer Fresnel Annulus [0.86, 0.98]
    // NEVER draw lines across the interior (r < 0.82) of the sphere!
    var direction_stripe = vec3<f32>(0.0);
    if (bubble.shape_kind == 0u && (bubble.feature_flags & FLAG_FACING_STRIPE) != 0u) {
        let heading_screen = normalize(vec2<f32>(dot(heading_dir, cam_right), dot(heading_dir, cam_up)));
        let angle_align = dot(disk_dir, heading_screen);

        let rim_annulus = smoothstep(0.85, 0.92, r) * (1.0 - smoothstep(0.97, 1.0, r));
        let angular_arc = smoothstep(0.60, 0.95, angle_align);
        let pulse = sin(globals.time * 5.0 - (1.0 - angle_align) * 4.0) * 0.25 + 0.75;
        let nose_accent = angular_arc * rim_annulus * (1.4 + 0.6 * pulse);

        direction_stripe = mix(bubble.team_color.rgb, vec3<f32>(1.0, 1.0, 1.0), 0.40) * nose_accent * 2.8;
    }

    // 5. Target-Lock Weapon Aperture on Glass Surface
    var aim_glow = vec3<f32>(0.0);
    if (bubble.is_attacking == 1u && (bubble.feature_flags & FLAG_ATTACK_APERTURE) != 0u) {
        let aim_dir = vec3<f32>(cos(bubble.aim_angle_rad), 0.0, sin(bubble.aim_angle_rad));
        let aim_dot = dot(normal, aim_dir);
        let ring_aperture = smoothstep(0.035, 0.008, abs(aim_dot - 0.72));
        let core_aperture = smoothstep(0.94, 0.99, aim_dot);
        aim_glow = vec3<f32>(1.0, 0.35, 0.15) * (ring_aperture * 3.2 + core_aperture * 1.8);
    }

    // 6. Progressive Damage Cracks & Electrical Arcing
    var crack_glow = vec3<f32>(0.0);
    if ((bubble.feature_flags & FLAG_CRACKS) != 0u && bubble.hp_ratio < 0.75) {
        let crack_mask = voronoi_cracks(in.uv * 3.0);
        if (bubble.hp_ratio < 0.35) {
            let flicker = sin(globals.time * 60.0) * 0.5 + 0.5;
            crack_glow = vec3<f32>(1.0, 0.25, 0.05) * crack_mask * (1.5 + flicker * 2.0);
        } else {
            video_color *= (1.0 - crack_mask * 0.7);
        }
    }

    // 7. Whole-Bubble Luminous Hit Surges
    var shield_surge = vec3<f32>(0.0);
    if (bubble.shield_hit_flash > 0.0) {
        let shield_pulse = sin(globals.time * 50.0) * 0.15 + 0.85;
        shield_surge = vec3<f32>(0.2, 0.85, 1.0) * bubble.shield_hit_flash * shield_pulse * (2.8 + pow(r, 2.5) * 4.0);
    }

    var hull_surge = vec3<f32>(0.0);
    if (bubble.hull_hit_flash > 0.0) {
        let hull_pulse = sin(globals.time * 60.0) * 0.20 + 0.80;
        hull_surge = vec3<f32>(1.0, 0.32, 0.08) * bubble.hull_hit_flash * hull_pulse * (3.6 + pow(r, 2.0) * 3.2);
    }

    // 8. Tactical Selection Outer Rim Aura
    var selection_glow = vec3<f32>(0.0);
    let is_selected = ((bubble.feature_flags & FLAG_SELECTED) != 0u) || (bubble.selected == 1u);
    if (is_selected) {
        let pulse = 0.85 + 0.15 * sin(6.0 * globals.time);
        selection_glow = vec3<f32>(0.25, 0.85, 1.0) * pulse * pow(r, 4.0) * 3.5;
    }

    // 9. Final Composite & Cloak Interference
    var final_rgb = video_color + rim_color + vec3<f32>(specular) + direction_stripe + aim_glow + crack_glow + shield_surge + hull_surge + selection_glow;
    var final_alpha = 0.90 + F * 0.10;

    let is_cloaked = ((bubble.feature_flags & FLAG_CLOAKED) != 0u) || (bubble.cloaked == 1u);
    if (is_cloaked) {
        final_alpha = 0.15 + F * 0.25;
        final_rgb += vec3<f32>(0.1, 0.3, 0.5) * sin(globals.time * 10.0 + in.world_position.y * 5.0) * 0.5;
    }

    return vec4<f32>(final_rgb, final_alpha);
}
```

---

### 5.2 Mist Wake Shader Specification: [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl)

The complete revised fragment shader for [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl):

```wgsl
// assets/shaders/mist_wake.wgsl
// Real Volumetric Curl-Warped Mist Wake & Atmospheric Perspiration Shader for Space Scuffle.
// Implements:
// - 2D Rotational FBM (mat2x2 rotation between octaves to eliminate grid stripes!)
// - 2-Stage Domain Warping for turbulent boiling fog
// - Organic noise erosion (eliminating all circular disc borders!)
// - Silky velocity-sheared slipstream filaments
// - Stochastic iridescent micro-bubbles
// - Additive bioluminescent HDR color grading (Cyan + Magenta + Ice-White + Golden Amber)

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
fn hash2(p: vec2<f32>) -> vec2<f32> {
    let q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)));
    return fract(sin(q) * 43758.5453);
}

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

// Rotational FBM Matrix (36.87 degrees rotation between octaves to eliminate grid stripes!)
const ROT: mat2x2<f32> = mat2x2<f32>(0.80, -0.60, 0.60, 0.80);

fn fbm_rot(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pos = p;
    for (var i = 0; i < 4; i++) {
        v += a * noise2d(pos);
        pos = ROT * pos * 2.02 + vec2<f32>(17.3, 31.7);
        a *= 0.5;
    }
    return v;
}

// 2-Stage Domain Warping for Organic Boiling Fog
fn curl_pattern(p: vec2<f32>, time: f32) -> f32 {
    let t = time * 0.15;
    let q = vec2<f32>(
        fbm_rot(p + vec2<f32>(t * 0.4, t * 0.2)),
        fbm_rot(p + vec2<f32>(5.2 - t * 0.3, 1.3 + t * 0.15))
    );
    let r = vec2<f32>(
        fbm_rot(p + 2.5 * q + vec2<f32>(1.7, 9.2 + t * 0.2)),
        fbm_rot(p + 2.5 * q + vec2<f32>(8.3, 2.8 - t * 0.2))
    );
    return fbm_rot(p + 2.5 * r);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // 1. Center and rotate coordinates along trail vector
    let uv = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
    let r2 = dot(uv, uv);
    if (r2 > 1.0) {
        discard;
    }

    let angle = mist.base_color.w;
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let rotated_p = vec2<f32>(
        uv.x * cos_a - uv.y * sin_a,
        uv.x * sin_a + uv.y * cos_a
    );

    let along_trail = rotated_p.x;   // Negative = forward, Positive = trailing wake
    let across_trail = rotated_p.y;  // Lateral distance from center axis

    // 2. Domain-Warped Turbulence & Organic Noise Erosion
    // Uses ROT to ensure ZERO horizontal/vertical grid stripes
    let noise_coord = vec2<f32>(along_trail * 1.8, across_trail * 2.4) + vec2<f32>(mist.seed * 19.3, mist.seed * 37.7);
    let turbulence = curl_pattern(noise_coord, globals.time);

    // Erode the base radial falloff with curl noise (eliminating circular disc outlines!)
    let base_falloff = clamp(1.0 - r2, 0.0, 1.0);
    let cloud_density = smoothstep(0.12, 0.85, turbulence * pow(base_falloff, 0.65));

    // 3. Silky Velocity-Sheared Slipstream Filaments (Moving bubbles only)
    var streaks = 0.0;
    if (abs(angle) > 0.001 || mist.aspect > 1.05) {
        let streak_pos = vec2<f32>(along_trail * 2.0 - globals.time * 2.5, across_trail * 4.0 + turbulence * 1.5);
        streaks = pow(fbm_rot(streak_pos), 2.0) * smoothstep(0.1, 0.7, smoothstep(1.0, -0.4, along_trail));
    }

    // 4. Stochastic Iridescent Micro-Droplets (Procedural Voronoi jitter)
    var bubble_intensity = 0.0;
    var bubble_rainbow = vec3<f32>(0.0);
    if (mist.speed_boosted != 1u) {
        let cell_coord = vec2<f32>(along_trail * 3.5, across_trail * 4.5) + vec2<f32>(mist.seed * 19.3, globals.time * 0.08);
        let cell_id = floor(cell_coord);
        let cell_uv = fract(cell_coord) - 0.5;
        let cell_hash = hash2(cell_id + mist.seed);
        let jitter_offset = (cell_hash - 0.5) * 0.5;
        let droplet_dist = length(cell_uv - jitter_offset);
        let droplet_ring = abs(droplet_dist - 0.18);

        if (droplet_ring < 0.05 && droplet_dist < 0.24) {
            bubble_intensity = smoothstep(0.05, 0.01, droplet_ring) * (1.0 - mist.age);
            let hue = fract(cell_hash.x * 7.0 + globals.time * 0.2);
            bubble_rainbow = vec3<f32>(
                0.5 + 0.5 * cos(6.28 * (hue + 0.0)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.33)),
                0.5 + 0.5 * cos(6.28 * (hue + 0.67))
            );
        }
    }

    // 5. Rich Multicolored Iridescent Color Composition (Gemini Reference Benchmark)
    let col_cyan    = vec3<f32>(0.12, 0.88, 1.0);
    let col_magenta = vec3<f32>(0.85, 0.22, 1.0);
    let col_ice     = vec3<f32>(0.92, 0.96, 1.0);
    let col_amber   = vec3<f32>(1.0, 0.58, 0.14);
    let col_gold    = vec3<f32>(1.0, 0.82, 0.30);

    var mist_color = mix(col_cyan, col_magenta, smoothstep(0.25, 0.70, turbulence));
    mist_color = mix(mist_color, col_ice, smoothstep(0.70, 0.95, turbulence));
    mist_color = mix(mist_color, col_amber, smoothstep(0.30, 0.80, abs(across_trail)));
    mist_color += streaks * col_cyan * 1.6;
    mist_color += bubble_intensity * bubble_rainbow * 2.5;

    // Overdrive mode shifts toward golden-orange thruster fire
    if (mist.speed_boosted == 1u) {
        mist_color = mix(mist_color, col_amber * 1.8, 0.6);
    }

    // 6. Additive Alpha & Lifetime Feathering
    let life_fade = smoothstep(1.0, 0.65, mist.age) * smoothstep(0.0, 0.10, mist.age);
    let final_density = (cloud_density * 0.85 + streaks * 0.35) * life_fade;
    let alpha = clamp(final_density * 0.65, 0.0, 0.85);

    if (alpha < 0.01) {
        discard;
    }

    // Output vibrant HDR color without alpha square-attenuation:
    return vec4<f32>(mist_color * alpha * 2.2, alpha);
}
```

---

### 5.3 Rust Kinematics Specification: [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs)

The revised `emit_mist_wake` function in [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs):

```rust
// In crates/client/src/vfx.rs

use std::sync::atomic::{AtomicU32, Ordering};

static EMIT_COUNTER: AtomicU32 = AtomicU32::new(1);

/// Spawns surface perspiration mist (when stationary) or aerodynamic air-suction wake particles (when moving).
pub fn emit_mist_wake(
    commands: &mut Commands<'_, '_>,
    assets: &VfxSharedAssets,
    mist_materials: &mut Assets<MistWakeMaterial>,
    bubble_pos: Vec3,
    velocity: Vec2,
    radius: f32,
    speed_boosted: bool,
    is_lab_preview: bool,
) {
    let speed = velocity.length();
    let emit_id = EMIT_COUNTER.fetch_add(1, Ordering::Relaxed);
    // Golden ratio hash generates high-entropy pseudo-random salt across 0.0..1.0
    let seed_salt = ((emit_id as f32) * 0.6180339887).fract();

    if speed <= 0.005 {
        // Surface Perspiration ("Sweating Orbs") on stationary bubble
        // Emit 2 soft, large, low-opacity vapor veils distributed smoothly across 360 degrees
        for i in 0..2 {
            let angle = (seed_salt * std::f32::consts::TAU + (i as f32) * std::f32::consts::PI).fract() * std::f32::consts::TAU;
            let radial_offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * (radius * 0.25);
            let spawn_pos = bubble_pos + radial_offset + Vec3::new(0.0, radius * 0.05, 0.0);

            let thermal_rise = Vec3::new(angle.cos() * 0.04, 0.08, angle.sin() * 0.04);
            let initial_scale = radius * 2.2;
            let target_scale = radius * 2.8;

            let mat = mist_materials.add(MistWakeMaterial {
                uniform: MistWakeUniformGpu {
                    base_color: Vec4::new(0.12, 0.88, 1.0, 0.0),
                    age: 0.0,
                    speed_boosted: u32::from(speed_boosted),
                    seed: (seed_salt + (i as f32) * 0.37).fract(),
                    aspect: 1.0,
                },
            });

            commands.spawn((
                Mesh3d(assets.mist_mesh.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(spawn_pos).with_scale(Vec3::splat(initial_scale)),
                MistWakeParticle {
                    kind: MistParticleKind::VaporPuff,
                    velocity: thermal_rise,
                    lifetime: Timer::from_seconds(1.6, TimerMode::Once),
                    initial_scale,
                    target_scale,
                },
            ));
        }
        return;
    }

    // Moving bubble: Aerodynamic Air Suction Slipstream
    let world_speed = if speed < 0.5 { speed * 25.0 } else { speed };
    let forward_dir = Vec3::new(velocity.x, 0.0, velocity.y).normalize();
    let trail_dir = -forward_dir;
    let right_dir = Vec3::new(-trail_dir.z, 0.0, trail_dir.x);
    let up_dir = Vec3::Y;

    let forward_screen = Vec2::new(velocity.x, -velocity.y).normalize_or_zero();
    let trail_screen = -forward_screen;
    let flow_angle = -trail_screen.y.atan2(trail_screen.x);

    let lab_conveyor = if is_lab_preview {
        trail_dir * (world_speed * 0.65)
    } else {
        Vec3::ZERO
    };

    // 4 spherical surface zones: Core, Port, Starboard, Crest
    let zones = [
        (Vec3::ZERO, trail_dir * (radius * 0.25)),
        (right_dir * (radius * 0.40), trail_dir * (radius * 0.15)),
        (-right_dir * (radius * 0.40), trail_dir * (radius * 0.15)),
        (up_dir * (radius * 0.35), trail_dir * (radius * 0.10)),
    ];

    for (i, &(lateral_vec, trail_offset)) in zones.iter().enumerate() {
        let spawn_pos = bubble_pos + lateral_vec + trail_offset;
        let suction_inward = -lateral_vec / 0.28;
        let forward_suction = forward_dir * (world_speed * 0.18);
        let velocity_vec = lab_conveyor + suction_inward + forward_suction;

        let initial_scale = radius * 2.0;
        let target_scale = radius * 1.1;

        let seed = (seed_salt + (i as f32) * 0.23).fract();

        let mat = mist_materials.add(MistWakeMaterial {
            uniform: MistWakeUniformGpu {
                base_color: Vec4::new(0.12, 0.88, 1.0, flow_angle),
                age: 0.0,
                speed_boosted: u32::from(speed_boosted),
                seed,
                aspect: 1.25,
            },
        });

        commands.spawn((
            Mesh3d(assets.mist_mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(spawn_pos).with_scale(Vec3::splat(initial_scale)),
            MistWakeParticle {
                kind: MistParticleKind::VaporPuff,
                velocity: velocity_vec,
                lifetime: Timer::from_seconds(0.85, TimerMode::Once),
                initial_scale,
                target_scale,
            },
        ));
    }
}
```

And in [`crates/client/src/vfx.rs:71-74`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs#L71-L74), update `depth_bias` for `MistWakeMaterial`:
```rust
impl Material for MistWakeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/mist_wake.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn depth_bias(&self) -> f32 {
        -45.0 // Ensures crystal bubbles and selection rings always render sharply in front of mist!
    }
}
```

---

### 5.4 Scene Controller Continuous Emission: [`crates/client/src/scene_controller.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs)

In [`crates/client/src/scene_controller.rs:423-447`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs#L423-L447) and [`579-598`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs#L579-L598):
When a unit is moving, emit mist **every frame** (`% 1 == 0`) so that there are never discrete stepped gaps along the trailing wake ribbon:
```rust
let is_moving = lab_state.speed > 0.05;
let throttled = if is_moving {
    true // Emit every frame when moving for a gapless, silky ribbon!
} else {
    lab_frame % 8 == 0 // Ethereal perspiration intervals when stationary
};
```
And in `sync_live_battle_system`:
```rust
let moving = instance.velocity[0].abs() + instance.velocity[1].abs() > 0.0005;
let throttled = if moving {
    true // Gapless trail in live battle
} else {
    (snapshot.tick.0 + instance.entity_id.0) % 8 == 0
};
```

---

## 6. Implementation Roadmap & Stage 2 Execution Plan

In adherence to Section 6 of [`AGENTS.md`](file:///home/cl/projects/space-scuffle/AGENTS.md), this architectural plan serves as the binding blueprint for Stage 2 implementation:

| Step | Target File | Action | Key Verification |
|---|---|---|---|
| **Step 1** | [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl) | Replace fragment shader with perspective-correct horizon math ($r = \sqrt{1 - NdotV^2}$), true camera view-plane basis, studio crescent rim reflections, and rim-confined facing accent. Remove interior white arch (`meridian_stripe`). | $r = 1.0$ at all silhouette points; center glass $r < 0.65$ has zero specular blob. |
| **Step 2** | [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl) | Replace shader with Rotational FBM (`mat2x2(0.8, -0.6, 0.6, 0.8)`), curl-noise erosion, non-circular silhouettes, velocity-sheared streaks, stochastic droplets, and luminous additive HDR output ($2.2 \times \text{color} \times \alpha$). | Zero parallel stripes, zero grid dots, zero button disc boundaries. |
| **Step 3** | [`crates/client/src/vfx.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/vfx.rs) | Replace static `seed_val` with atomic counter + golden ratio hash. Spawn 360-degree soft vapor veils when stationary ($2.2R \to 2.8R$). Update moving wake with $0.28\text{s}$ suction inward, scale contraction ($2.0R \to 1.1R$), and set `depth_bias: -45.0`. | Stationary bubbles sweat uniformly in all directions; moving wakes neck smoothly behind the sphere. |
| **Step 4** | [`crates/client/src/scene_controller.rs`](file:///home/cl/projects/space-scuffle/crates/client/src/scene_controller.rs) | Update emission throttling: moving units emit every frame for gapless wakes; stationary units emit every 8 frames. | Continuous, silky ribbon wake trailing behind moving units without stepped gaps. |
| **Step 5** | Verification & Quality Gates | Run full workspace test suite: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. | All tests pass, zero warnings, clean formatting. |

---

## 7. Verification & Testing Matrix

1. **Mathematical Invariance Verification:**
   - **Silhouette Invariance:** Under any camera pitch ($\theta \in [15^\circ, 85^\circ]$) and yaw ($\phi \in [0, 360^\circ]$), the outer boundary $r = \sqrt{1.0 - (\vec{N} \cdot \vec{V})^2} = 1.0$ forms a mathematically perfect circle in screen space.
   - **Center Aperture Transparency:** For all fragments with $r \le 0.60$, specular glare and directional stripes are strictly $0.0$, ensuring character animation flipbooks are 100% visible and unblemished.
2. **Stationary Mist Distribution Test:**
   - Verify that stationary mist particles spawn across all four quadrants ($\theta \in [0, 2\pi)$) rather than piling up at fixed upper-left angles.
3. **No-Stripe Shader Validation:**
   - Verify that the rotational matrix `mat2x2<f32>(0.8, -0.6, 0.6, 0.8)` eliminates Cartesian noise alignment at all camera angles.
4. **Automated Quality Gates:**
   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --all -- --check
   ```
