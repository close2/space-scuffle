# Plan 20: Screen-Space Circular Bubble Silhouette, Anamorphic Tilt Correction, and Enhanced Mist Density

**Document Status:** Architectural Blueprint (Stage 1)  
**Author:** Principal Graphics & Volumetric Shader Architect (`pro` model)  
**Target Crates:** [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl), [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl)  
**Binding Standards:** [`AGENTS.md`](file:///home/cl/projects/space-scuffle/AGENTS.md) (Textbook-Grade Engineering, Zero Placeholders, Strict Invariants)  
**Visual Artifacts Under Diagnosis:**
- [User Screenshot 1: Corner Elliptical Stretching `media_1793033681696_4347f33a.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793033681696_4347f33a.png)
- [User Screenshot 2: Center Round Bubbles & Brief Overlap `media_1793033848500_8d4ae0e7.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793033848500_8d4ae0e7.png)

---

## 1. Executive Summary & Forensic Analysis

### 1.1 The User's Feedback
> *"bubbles are still not round (see screenshot)*  
> *The mist is \*much\* better. Please make it a little bit stronger, but otherwise we can keep it for now.*  
> *I have noticed that for short periods units occupy the same space (see second screenshot).*  
> *If this is the same behavior as in sc2, that's fine. The 2 units are then pushed aside."*

This feedback presents two visual phenomena and one simulation mechanics question:
1. **Corner Bubble Anamorphism:** When units move into the corners of the viewport (Screenshot 1), the bubbles stretch into diagonal ellipses, and the characters inside tilt diagonally. In contrast, units near the center of the viewport (Screenshot 2) render as round spheres.
2. **Mist Vibrancy:** The mist fluid mechanics, domain-warping, and coloration are validated as *"much better"*, but require an approximate $50\%$ boost in density and luminous vibrancy.
3. **Unit Convergence Overlap:** Brief penetration of soft unit radii during convergence before being smoothly separated matches StarCraft II's authoritative push priority mechanics.

---

## 2. Forensic Diagnosis: Why 3D Spheres Form Corner Ellipses in Perspective Projection

### 2.1 The Conic Section Geometry of Perspective Frustums
In 3D computer graphics under a standard planar perspective projection matrix (`PerspectiveProjection`):
1. A 3D sphere of radius $R$ centered at $\vec{C} = (X_c, Y_c, Z_c)$ in camera space subtends a cone of sight rays from the camera's center of projection $\vec{O} = (0, 0, 0)$ tangent to the sphere.
2. The flat image sensor (the monitor screen) is a plane perpendicular to the camera's optical axis ($\vec{Z}_{\text{cam}}$).
3. When the sphere is centered along the optical axis ($\vec{C} = (0, 0, Z_c)$), the tangent cone is a right circular cone whose axis is perpendicular to the image plane. The intersection of this cone with the plane is a **circle**.
4. When the sphere is located off-axis (e.g. in the bottom-right corner of the screen, where $\theta = \angle(\vec{C}, \vec{Z}_{\text{cam}}) > 0$), the tangent cone intersects the planar image sensor at an oblique angle.
5. In geometry, the planar section of a cone with an oblique plane is **a conic section: an ellipse**!
   The semi-major axis of the ellipse is aligned with the radial direction from the screen center to the sphere projection, and is stretched by a factor of approximately $\frac{1}{\cos\theta}$ (or $\frac{1}{\cos^2\theta}$).

```
                             OFF-AXIS PERSPECTIVE CONE:
                                  \
                                   \      ( 3D Sphere )
                                    \    /             \
                                     \  |   Center C    |
                                      \  \             /
                                       \  '-----------'
    FLAT SCREEN PLANE                   \        /
    [ Perpendicular to Z_cam ]           \      /  Tangent Cone
    +-------------------------+           \    /
    |                         |            \  /
    |       Screen Center     |             \/
    |             x           |             /\
    |                          \           /  \
    |                           \         /    \
    |     Ellipse Intersection:  \_______/      \
    |     Stretched radially!     |     |        \
    +-----------------------------+-----+---------+
                                 /       \
                                /         \
                               Camera Eye Origin (0,0,0)
```

Because Bevy rasterizes the physical 3D mesh triangles of `bevy_render::mesh::Sphere`, the rasterizer produces fragments covering the entire oblique elliptical footprint on the monitor. Even if the internal shader calculations are spherical, the *outer contour of pixels drawn by the GPU* is an ellipse!

### 2.2 Why Characters Inside Corner Bubbles Tilt Diagonally
In [`assets/shaders/bubble_dome.wgsl:82-84`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl#L82-L84):
```wgsl
let sphere_center = in.world_position.xyz - normal * bubble.radius;
let cam_forward = normalize(sphere_center - view.world_position);
let cam_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), cam_forward));
let cam_up = cross(cam_forward, cam_right);
```
- The RTS camera is pitched down by $58^\circ$ from vertical (`vec3(0, 1, 0)`).
- When a unit is in the bottom-right corner of the screen, the vector `cam_forward` has large positive $X$ and large negative $Y$ components.
- Computing $\text{cross}((0, 1, 0), \vec{V}_{\text{ray}})$ forces `cam_right` to lie in the world horizontal plane ($Y = 0$), rather than the camera's viewport horizontal axis!
- Under a $58^\circ$ pitch, projecting a horizontal world vector onto an off-axis camera plane rotates `cam_right` relative to the screen's true horizontal axis by up to $35^\circ$!
- Because `lens_uv` is indexed by `disk = vec2(dot(normal, cam_right), -dot(normal, cam_up))`, the video flipbook texture of the character is sampled with a $35^\circ$ rotational slant, causing the character to lean diagonally in corner bubbles.

---

## 3. Mathematical Solution: Screen-Space Perfect Circles & Zero-Tilt Basis

### 3.1 Exact Screen-Space Circle Geometry
To make mobile unit bubbles appear as **100% mathematically perfect, un-skewed 1:1 circles** across every pixel of the monitor (center, edges, and corners):
1. We obtain the camera's true screen-aligned orthonormal basis directly from Bevy's `view.world_from_view` matrix:
   ```wgsl
   let screen_right = normalize(view.world_from_view[0].xyz);
   let screen_up = normalize(view.world_from_view[1].xyz);
   let screen_forward = -normalize(view.world_from_view[2].xyz);
   ```
   These three vectors are constant across the entire viewport. `screen_right` is parallel to the physical rows of pixels on the monitor, and `screen_up` is parallel to the physical columns of pixels.
2. For mobile unit spheres (`bubble.shape_kind == 0u`), let:
   - $\vec{p}_{\text{rel}} = \vec{P}_{\text{frag}} - \vec{P}_{\text{cam}}$ (vector from camera to current fragment)
   - $\vec{c}_{\text{rel}} = \vec{C}_{\text{sphere}} - \vec{P}_{\text{cam}}$ (vector from camera to sphere center)
   - $z_p = \vec{p}_{\text{rel}} \cdot \vec{d}_{\text{forward}}$ (depth of fragment along optical axis)
   - $z_c = \vec{c}_{\text{rel}} \cdot \vec{d}_{\text{forward}}$ (depth of sphere center along optical axis)
3. Project the ray through this fragment onto the camera plane at depth $z_c$:
   $$\vec{P}_{\text{screen}} = \vec{p}_{\text{rel}} \left(\frac{z_c}{z_p}\right) - \vec{c}_{\text{rel}}$$
4. Decompose $\vec{P}_{\text{screen}}$ into the camera's screen-space coordinates $(x_s, y_s)$ normalized by `bubble.radius`:
   ```wgsl
   let screen_disk = vec2<f32>(
       dot(p_screen_at_zc, screen_right),
       -dot(p_screen_at_zc, screen_up)
   ) / max(bubble.radius, 0.001);
   ```
5. **The Geometric Covering Invariant:**
   At depth $z_c$, the 3D sphere has physical radius $R$. Because perspective projection stretches the sphere into an ellipse whose minor semi-axis is $\ge R$ and major semi-axis is $> R$, the 3D sphere mesh's rasterized footprint **completely envelops and covers the true screen-space circle** `dot(screen_disk, screen_disk) <= 1.0`.
6. **Clipping the Perspective Elliptical Ears:**
   By discarding fragments outside the unit disk:
   ```wgsl
   if (bubble.shape_kind == 0u && dot(screen_disk, screen_disk) > 1.0) {
       discard;
   }
   ```
   and setting:
   ```wgsl
   let disk = screen_disk;
   let r2 = dot(screen_disk, screen_disk);
   let r = sqrt(r2);
   ```
   every mobile unit bubble rendered on the display is guaranteed to be a **pixel-perfect 1:1 geometric circle** with zero oval elongation, anywhere on screen!
7. **Zero-Tilt Character Orientation:**
   Because `screen_right` and `screen_up` are the camera's true viewport axes, indexing `lens_uv` with `disk` guarantees that the character inside is **100% upright with zero tilt** in the center and in the extreme corners.

### 3.2 Building Domes Invariant
For grounded building hemisphere domes (`bubble.shape_kind == 1u`), they must remain grounded to the terrain grid. For `shape_kind == 1u`, the shader retains the 3D hemisphere normal projection with `screen_right` and `screen_up`, ensuring building domes sit flat on the terrain without clipping their ground contact rims.

---

## 4. StarCraft II Unit Push & Overlap Validation

In Screenshot 2 ([`media_1793033848500_8d4ae0e7.png`](file:///home/cl/.gemini/antigravity/brain/28ee1670-e5dd-4945-94d2-278c6b003750/.user_uploaded/media_1793033848500_8d4ae0e7.png)), the user observed:
> *"I have noticed that for short periods units occupy the same space (see second screenshot).*  
> *If this is the same behavior as in sc2, that's fine. The 2 units are then pushed aside."*

### Architectural Confirmation
**Yes, this precisely matches StarCraft II's authoritative collision physics:**
1. In StarCraft II, ground units do not have infinite-mass rigid physics colliders. Instead, they use a dual-radius system:
   - **Hard Footprint Radius:** Used for static building placement and NavMesh pathing.
   - **Soft Separation Radius:** Used for friendly dynamic unit steering.
2. When multiple units are issued a move or attack order toward the same coordinate (or pass through a narrow ramp/chokepoint), units temporarily overlap their soft separation radii.
3. Over the subsequent ticks, SC2's `PushPriority` and separation solver applies an outward relaxation velocity:
   - Higher-tier or stationary/attacking units have higher push priority.
   - Moving or lower-priority units yield and are pushed aside tangential to the movement vector.
4. In Space Scuffle's `crates/sim_core/src/world.rs`, this is implemented via:
   - `PUSH_RELAXATION = 0.35` (Baumgarte penetration relaxation per tick).
   - `PushPriority` hierarchy (HoldPosition / Attacking units resist pushes; moving units yield).
   - Strict speed clamping preventing pushed units from vibrating or exceeding maximum speed.

Therefore, the brief overlap during convergence followed by smooth separation observed by the user is the **intended, textbook-grade RTS behavior**.

---

## 5. Mist Density & Vibrancy Enhancement Specification

The user requested: *"The mist is \*much\* better. Please make it a little bit stronger, but otherwise we can keep it for now."*

In [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl):
1. **Wisp Density & Body:**
   Widen the wisp threshold in line 86 from `smoothstep(0.30, 0.82, ...)` to:
   ```wgsl
   let wisp_structure = smoothstep(0.24, 0.80, w * 0.60 + billow * 0.40);
   ```
   This gives the vapor clouds slightly more body and fuller volume.
2. **Spectral Color Amplification:**
   Multiply `spectral_color` by $1.35$ in line 114:
   ```wgsl
   spectral_color *= 1.35;
   ```
3. **Additive Alpha Boost:**
   Increase the alpha density factor from $0.28$ to $0.44$, and the upper clamp from $0.30$ to $0.46$:
   ```wgsl
   let alpha = clamp(vapor_density * life_fade * 0.44, 0.0, 0.46);
   ```
   This boosts perceived mist presence by $\approx 55\%$ while preserving the soft, luminous, additive character without blowing out into solid white.

---

## 6. Exact WGSL Code Specifications

### 6.1 `assets/shaders/bubble_dome.wgsl` (Fragment Basis & Clipping)

Replace lines 76–105 of [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl) with:

```wgsl
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let view_dir = normalize(view.world_position - in.world_position.xyz);
    let normal = normalize(in.world_normal);

    // 1. Camera View-Plane Basis (Constant across viewport from view.world_from_view matrix)
    let screen_right = normalize(view.world_from_view[0].xyz);
    let screen_up = normalize(view.world_from_view[1].xyz);
    let screen_forward = -normalize(view.world_from_view[2].xyz);

    // Exact sphere center in world space
    let sphere_center = in.world_position.xyz - normal * bubble.radius;
    let p_rel = in.world_position.xyz - view.world_position;
    let c_rel = sphere_center - view.world_position;

    // Optical depth along camera optical axis
    let z_p = max(dot(p_rel, screen_forward), 0.001);
    let z_c = max(dot(c_rel, screen_forward), 0.001);

    // Project fragment onto camera plane at depth z_c (plane through sphere center perpendicular to screen_forward)
    let p_screen_at_zc = p_rel * (z_c / z_p) - c_rel;
    let screen_disk = vec2<f32>(
        dot(p_screen_at_zc, screen_right),
        -dot(p_screen_at_zc, screen_up)
    ) / max(bubble.radius, 0.001);

    var disk: vec2<f32>;
    var r2: f32;
    var r: f32;

    if (bubble.shape_kind == 0u) {
        // Mobile Units: Discard perspective-stretched elliptical ears outside true 1:1 screen-space circle!
        let screen_r2 = dot(screen_disk, screen_disk);
        if (screen_r2 > 1.0) {
            discard;
        }
        disk = screen_disk;
        r2 = screen_r2;
        r = sqrt(r2);
    } else {
        // Building Domes: Grounded 3D hemisphere projection
        disk = vec2<f32>(dot(normal, screen_right), -dot(normal, screen_up));
        let n_dot_v = max(dot(normal, view_dir), 0.0);
        r2 = clamp(1.0 - n_dot_v * n_dot_v, 0.0, 1.0);
        r = sqrt(r2);
    }

    let disk_dir = select(vec2<f32>(1.0, 0.0), disk / max(length(disk), 0.0001), length(disk) > 0.0001);

    // Optical glass lens refraction and magnification
    let z_lens = sqrt(max(1.0 - r2, 0.0));
    var lens_uv = vec2<f32>(0.5, 0.5) + disk * mix(0.48, 0.35, z_lens);

    // Facing-aware horizontal mirroring (aligned with screen_right)
    let heading_dir = vec3<f32>(cos(bubble.heading_angle_rad), 0.0, sin(bubble.heading_angle_rad));
    if (dot(heading_dir, screen_right) < 0.0) {
        lens_uv.x = 1.0 - lens_uv.x;
    }

    // Lens aperture vignette & edge depth darkening (kicks in only near glass rim r > 0.85)
    let lens_mask = 1.0 - smoothstep(0.88, 0.99, r);
    let glass_depth_tint = 1.0 - smoothstep(0.80, 0.99, r) * 0.40;
```

And in lines 147–183:
Replace `cam_right` / `cam_up` / `cam_forward` references with `screen_right` / `screen_up` / `screen_forward` to guarantee studio highlights and rim accents are perfectly upright across the screen.

---

### 6.2 `assets/shaders/mist_wake.wgsl` (Enhanced Density & Vibrancy)

In [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl):

```wgsl
    // 2. Wisp Structure with slightly broader volume
    let wisp_structure = smoothstep(0.24, 0.80, w * 0.60 + billow * 0.40);

    ...

    // Subtle cyan-white highlight on the sharpest turbulent ridges
    let ridge = billow * gaussian_core;
    spectral_color += col_core * (pow(ridge, 4.0) * 0.25);
    spectral_color *= 1.35; // 35% luminous vibrancy boost!

    ...

    // 5. Additive Alpha with ~55% density boost
    let life_fade = pow(clamp(1.0 - mist.age, 0.0, 1.0), 1.3) * smoothstep(0.0, 0.10, mist.age);
    let alpha = clamp(vapor_density * life_fade * 0.44, 0.0, 0.46);

    if (alpha < 0.002) {
        discard;
    }

    return vec4<f32>(spectral_color * alpha, alpha);
```

---

## 7. Implementation Roadmap & Quality Gates

| Step | Target File | Action | Key Verification |
|---|---|---|---|
| **Step 1** | [`assets/shaders/bubble_dome.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/bubble_dome.wgsl) | Implement screen-space disk projection, clip `screen_r2 > 1.0` for mobile units, use `screen_right`/`screen_up` for lens UV and studio specular. | Mobile units in viewport corners are 100% circular with zero character tilt. |
| **Step 2** | [`assets/shaders/mist_wake.wgsl`](file:///home/cl/projects/space-scuffle/assets/shaders/mist_wake.wgsl) | Apply wisp structure broadening, `spectral_color *= 1.35`, and alpha density boost ($0.28 \to 0.44$, max $0.46$). | Mist is noticeably richer, denser, and more vibrant without blowing out. |
| **Step 3** | Quality Gates & Compilation | Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. | All automated tests pass cleanly with zero warnings. |
