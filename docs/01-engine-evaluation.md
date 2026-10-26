# 01 — Engine & Tech Stack Evaluation

Before writing any code, we need to evaluate whether to use/fork an existing open-source RTS engine, use a general-purpose open-source 3D engine, or build a custom stack using modular libraries—and identify the exact technical traps we will encounter with each.

---

## 1. What Makes Our Game Uniquely Challenging for an Engine?

1. **The Bubble + Video Renderer:**
   - Up to **400–1,000 bubbles** on screen simultaneously.
   - Each bubble is a semi-transparent refractive/iridescent sphere displaying an independent video sequence, overlaid with procedural damage cracks, CRT glitch scanlines, directional rim indicators, ground selection rings, and trailing mist particles.
   - *Trap:* Standard video players (`ffmpeg` / hardware decoders) cannot decode 500 video streams at once. Standard alpha-blended 3D spheres in naive engines cause massive **alpha-sorting / overdraw bottlenecks** and break each other's depth buffers.
2. **Strict Cross-Platform Determinism (Linux + Windows + macOS):**
   - Since we chose **Deterministic Lockstep**, a client on Linux (`x86_64`) and a client on macOS (`Apple Silicon ARM64`) or Windows must compute the **exact same bit-for-bit simulation state** every tick.
   - *Trap:* Standard IEEE 754 floating-point math (`sin`, `cos`, `sqrt`, fused multiply-add `FMA`, or compiler optimizations) can diverge by 1ulp between Intel, AMD, and ARM CPUs, causing game desyncs after a few minutes. Most general-purpose game engines (Godot, Unity, Unreal, standard Bevy physics) are **not** deterministic across platforms.
3. **SC2-Specific Pathfinding & Combat Feel:**
   - Dynamic NavMesh carving when buildings are placed/destroyed, smooth funnel pathing, local steering (RVO/Boids), friendly push-priority, discrete cliff levels with high-ground fog of war, and `DamagePoint`/`Backswing` stutter-step weapon logic.
4. **Headless High-Speed Execution for the Bot API:**
   - Third-party bot developers will want to run the simulation **without a GPU or window** at 1,000+ ticks/sec. If game logic is tightly coupled to scene-graph nodes (like in Godot or Unreal), running headless at ultra-high speed is either impossible or slow.

---

## 2. Candidate Approaches Compared

### Option 1: Existing Open-Source RTS Engines

#### 1A. Recoil Engine (formerly Spring RTS — used by *Beyond All Reason*)
- **Language:** C++20 + Lua scripting.
- **What it gives you:** Battle-tested deterministic lockstep networking, flow-field + A* pathfinding for thousands of units, fog of war, replays, terrain heightmaps, and RTS UI framework.
- **Advantages:**
  - Years of RTS edge cases already solved.
- **Disadvantages & Problems We Will Encounter:**
  - **Wrong Gameplay Model:** Built for *Total Annihilation / Supreme Commander* style physics (ballistic trajectories, continuous terrain slopes rather than SC2 discrete cliff levels, turn-radius vehicle movement rather than snappy SC2 infantry micro).
  - **Rendering Architecture:** Uses a custom, heavily customized OpenGL 4 renderer designed for 3D unit meshes and terrain heightmaps; retrofitting our custom multi-layer Bubble + Video Texture Array shader and Stylized SC2/LoL cliff maps requires fighting 20 years of legacy C++ rendering code.
  - **macOS Support:** OpenGL 4 is deprecated/capped at 4.1 on macOS, which has historically been a major pain point for Spring/Recoil on Mac.

#### 1B. OpenRA
- **Language:** C#.
- **What it gives you:** Classic 2D sprite RTS engine (Command & Conquer / Red Alert / Dune 2000), lockstep networking, map editor, replays.
- **Disadvantages & Problems We Will Encounter:**
  - Fundamentally a **2D tile/cell-based engine** (units move cell-to-cell or on coarse sub-cells). It does not have SC2's continuous NavMesh/funnel pathing, smooth boid flocking, or modern 3D shader/lighting pipelines for stylized 3D environments and refractive 3D bubbles.

**Verdict on Existing RTS Engines:** Studying their source code (especially Recoil's networking/flow-fields and OpenRA's trait/activity architecture) is very valuable, but building *Space Scuffle* inside them will mean fighting their core assumptions.

---

### Option 2: Godot 4 + Rust (`gdext`) or C++ (GDExtension)
- **Architecture:** Keep the deterministic RTS simulation (`sim_core`) in a pure Rust library, and use **Godot 4** as the visual client, UI system, audio player, and Map Editor.
- **Advantages:**
  - **Built-in WYSIWYG Editor:** You immediately get a 3D scene editor, UI layout editor (great for RTS HUD, menus, lobby), particle system editor (for trailing mist and explosions), and animation timeline.
  - **Map Editor shortcut:** You can either build maps directly in the Godot Editor with custom plugins (`@tool` scripts) or build an in-game editor using Godot's rich UI nodes (`Control`, `Tree`, `ItemList`, `SubViewport`).
  - **Cross-Platform Export:** One-click export to Linux, Windows, macOS, and Web.
- **Disadvantages & Problems We Will Encounter:**
  - **Two-World Friction (Rust <-> Godot):** Every frame, the Rust simulation must sync positions, orientations, health, and animation states of 1,000 units across the FFI boundary into Godot's `RenderingServer` (`MultiMeshInstance3D`).
  - **Shader & Custom Render Pass Limits:** While Godot 4 has a nice shader language (similar to GLSL) and Compositor Effects, feeding per-instance custom data into `MultiMesh` is limited to `TRANSFORM` + `COLOR` (4 floats) + `CUSTOM_DATA` (4 floats) unless you pack additional state into a dynamic data texture (`ImageTexture`) updated every frame from Rust.
  - **Web Export Caveat:** `gdext` (Rust in Godot) Wasm support is still experimental compared to desktop targets.

---

### Option 3: Rust + Bevy Engine (Decoupled `sim_core` + Bevy Client)
- **Architecture:**
  - `sim_core`: A 100% pure, deterministic Rust crate (zero GPU/window dependencies). Runs the fixed-tick simulation, pathfinding, combat, lockstep commands, and Bot API.
  - `client`: A **Bevy** application that reads `sim_core` state, interpolates positions between ticks, and renders the 3D terrain, buildings, UI, particles, and instanced Bubble shaders via `wgpu`.
- **Advantages:**
  - **Single Language & Zero FFI Overhead:** Everything—simulation, rendering, networking, relay server, bot runner, and asset baker—is written in Rust and shares exact types directly in memory.
  - **First-Class Custom Shaders (`wgpu` + WGSL):** Full access to GPU storage buffers (`StorageBuffer`), Texture2D Arrays (`texture_2d_array`), instanced rendering, and custom post-processing—making it straightforward to pass rich per-bubble structs (`unit_type`, `anim_state`, `frame_idx`, `hp_ratio`, `shield_ratio`, `facing_angle`, `attack_angle`, `team_color`) directly to the GPU in a single draw call.
  - **Native Cross-Platform + WebGPU:** `wgpu` runs natively on Vulkan (Linux/Windows), DX12 (Windows), Metal (macOS), and WebGPU (Browser).
  - **Parallel Execution:** Bevy's scheduler and Rust's `rayon` make it easy to parallelize visibility/fog-of-war updates, steering, and particle updates across CPU cores.
- **Disadvantages & Problems We Will Encounter:**
  - **No Built-in Visual Scene Editor Yet:** Unlike Godot, you don't get a full drag-and-drop scene/particle editor out of the box. We must either use **Blender / Tiled** for authoring assets and maps, or build our own debug/editor panels using `bevy_egui` + `bevy_hanabi` (for particles).
  - **Bevy Version Updates:** Bevy releases major updates every ~3–4 months with API changes. *Mitigation:* Pin a stable Bevy version and keep all game rules inside `sim_core` (which doesn't depend on Bevy at all, or only uses standalone math/ECS), so only the rendering layer touches Bevy APIs.

---

### Option 4: Custom Rust Engine (`winit` + raw `wgpu` + `egui` + `glam`)
- **Architecture:** Same pure `sim_core`, but instead of Bevy, we write our own `wgpu` render pipelines from scratch, use `gltf` crate to load 3D buildings/terrain, `egui` for UI, and `kira` for audio.
- **Advantages:**
  - Zero framework churn; you understand 100% of the rendering pipeline.
  - An RTS has a fixed, predictable camera angle (top-down perspective) and a very specific set of render passes:
    1. Shadow map pass
    2. Terrain & static cliff/building pass
    3. Ground decals & selection rings pass
    4. Fog-of-War overlay pass
    5. Instanced Bubble & Video shader pass
    6. Particle / Beam / Projectile pass
    7. HUD / Minimap / UI pass
- **Disadvantages & Problems We Will Encounter:**
  - **Boilerplate Cost:** You have to write your own glTF mesh/material loader, shadow-mapping boilerplate, particle system, skeletal animation (if buildings have animations), and screen-space picking. That can easily add 4–8 weeks of low-level graphics plumbing before gameplay shines.

---

## 3. Feature-by-Feature Ease of Implementation Matrix

| Feature Requirement | Option 2: Godot 4 + Rust | Option 3: Bevy + Rust `sim_core` | Option 4: Raw `wgpu` + Rust |
| :--- | :--- | :--- | :--- |
| **Deterministic Lockstep Core** | Easy (in Rust crate) | Easy (in Rust crate) | Easy (in Rust crate) |
| **Headless 10,000x Bot Runner** | Easy (run Rust crate directly) | Easy (run Rust crate directly) | Easy (run Rust crate directly) |
| **1,000 Instanced Video Bubbles** | Medium-Hard (must pack state into textures for `MultiMesh` + `Texture2DArray`) | **Easy-Medium** (native WGSL `StorageBuffer` + `texture_2d_array`) | **Easy-Medium** (full control over instanced WGSL pipeline) |
| **Mist Trails, Lasers & Shatter VFX** | **Very Easy** (visual `GPUParticles3D` editor) | Medium (`bevy_hanabi` or custom ribbon/particle shaders in code) | Hard (must write GPU particle simulator from scratch) |
| **3D Terrain, Cliffs & Lighting** | **Very Easy** (visual viewport, built-in shadows/PBR/sky) | **Easy** (PBR, cascades shadows, bloom, glTF out of the box) | Hard (must code shadow cascades, PBR, glTF loading) |
| **RTS HUD, Minimap & Menus** | **Very Easy** (Godot `Control` GUI editor) | Medium (`bevy_ui` or `bevy_egui`) | Medium (`egui`) |
| **Map Editor** | **Easy** (Godot `@tool` mode or Godot GUI) | Medium (`bevy_egui` in-engine editor + Tiled import) | Medium (`egui` in-engine editor + Tiled import) |
| **Linux / Win / Mac / Web Portability** | High (Web `gdext` still maturing) | **Very High** (`wgpu` targets all 4 natively) | **Very High** (`wgpu` targets all 4 natively) |

---

## 4. Major Technical Problems & How We Solve Them Upfront

Regardless of whether we pick **Bevy + Rust** or **Godot + Rust**, we will hit these 4 classic RTS engineering traps unless we design for them from Day 1:

### Problem 1: Cross-Platform Floating-Point Desyncs
- **The Issue:** If Player A is on Linux (AMD Ryzen) and Player B is on macOS (M3 ARM), standard `f32` trigonometry or compiler auto-vectorization can produce `0.33333334` on one machine and `0.33333331` on the other. One unit barely misses a collision check on Player A's machine, and 30 seconds later the two games are completely different.
- **The Solution:**
  - **Fixed-Point Math (`I32F32` or `I48F16` via the Rust `fixed` crate)** for all simulation coordinates, velocities, radii, and cooldowns inside `sim_core`. Integer/fixed-point arithmetic is **100% bit-identical across every CPU architecture on earth** (x86, ARM, WebAssembly).
  - Only convert to `f32` when handing positions to the renderer!
  - Furthermore, every tick computes a fast `xxHash64` checksum of the simulation state and includes it in the lockstep network packet so any desync bug is caught on the exact tick it happens.

### Problem 2: Video Memory (VRAM) Explosion
- **The Issue:** Suppose we have 30 unit types, each with 4 animations (`Idle`, `Walk`, `Attack`, `Die`), 2 seconds each at 30 FPS (60 frames per animation), at `256x256` resolution.
  - Uncompressed RGBA8: `30 units × 4 anims × 60 frames × 256 × 256 × 4 bytes` = **~1.88 GB of VRAM** (and if 8 directions later: **~15 GB of VRAM**!).
- **The Solution:**
  1. **Resolution & Frame Rate:** Inside a bubble that occupies `64x64` to `180x180` pixels on a 1080p/1440p monitor, a `192x192` or `256x256` texture at `15–24 FPS` (with optional frame blending in the shader) is indistinguishable from 60 FPS 1080p—and actually fits the holographic/retro-CRT monitor aesthetic inside the bubble!
  2. **BC7 / ASTC GPU Block Compression (4:1 reduction):** Compressing texture arrays with BC7 drops `4 bytes/pixel` to `1 byte/pixel` directly in VRAM (`1.88 GB -> 470 MB`, or `~120 MB` at `192x192 @ 15 FPS`).
  3. **On-Demand Loading:** Only load the texture arrays for the factions/units actually present in the current match.

### Problem 3: Transparent Bubble Overdraw & Depth Sorting
- **The Issue:** Bubbles are semi-transparent glass/energy spheres containing an opaque/semi-transparent video plane inside. In 3D graphics, transparent objects don't write to the depth buffer normally; if 100 bubbles overlap, sorting and blending them back-to-front can cause flickering or slow frame rates.
- **The Solution:**
  - Render each bubble as a **single camera-facing billboarded sphere impostor (raymarched sphere in the fragment shader)** or a low-poly hemisphere with **depth-write enabled (`Alpha-to-Coverage` / opaque inner core + additive/premultiplied outer Fresnel rim)**.
  - Raymarching a perfect mathematical sphere inside a quad's fragment shader takes ~10 GPU instructions, gives **pixel-perfect smooth spheres at any zoom level** (zero polygon edges!), computes exact surface normals for refraction and glass cracks, and sorts cleanly by unit center depth!

### Problem 4: Units Getting Stuck on Each Other (The "Conga Line" Problem)
- **The Issue:** Naive `A*` pathfinding makes 50 units follow the exact same single-file line and jam at choke points.
- **The Solution:**
  - Combine **NavMesh + Funnel** (for small groups) or **Flow Fields** (when 20+ units share a destination) with **Boid Separation + Push Priority** so moving units smoothly part idle units and spread into concave arcs around targets.
