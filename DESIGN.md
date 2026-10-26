# Space Scuffle — Game Design & Technical Architecture

## Documentation Index
- **[01 — Engine & Tech Stack Evaluation](file:///home/cl/projects/space-scuffle/docs/01-engine-evaluation.md):** Detailed comparison of existing RTS engines (Recoil/Spring, OpenRA), Godot 4 + Rust, Bevy + Rust, and custom `wgpu`, plus solutions for the 4 major technical traps (cross-platform floating-point desyncs, VRAM explosion, transparent bubble sorting, and pathing gridlock).
- **[02 — Bubble Visual Design & Readability](file:///home/cl/projects/space-scuffle/docs/02-bubble-visual-design.md):** How unit bubbles and building hemisphere domes communicate movement, facing/turn rate, attack direction, team, unit class silhouette, and progressive damage/shattering.
- **[03 — StarCraft II Mechanics Reference](file:///home/cl/projects/space-scuffle/docs/03-sc2-mechanics-reference.md):** Complete unit data model, damage formulas, weapon timings (`DamagePoint`, `Backswing`, `Cooldown`), movement physics, high-ground vision rules, and SC2 limitations to keep vs. remove.
- **[04 — Pathfinding, Lockstep Networking, Bot API & Map Editor](file:///home/cl/projects/space-scuffle/docs/04-pathfinding-networking-bot-api.md):** CDT NavMesh + Funnel + Boid steering, Deterministic Lockstep over Relay Server, headless/real-time Bot API, Tiled/in-engine Map Editor, and the Parallel Track A / Track B roadmap.
- **[05 — Parallel Implementation Plans (Sub-Agent Blueprints)](file:///home/cl/projects/space-scuffle/docs/05-parallel-implementation-plans.md):** Concrete Cargo workspace structure, shared `sim_types` contract, and task-by-task blueprints for the 4 parallel sub-agent workstreams.

---

## 1. Vision & Core Concept
**Space Scuffle** is a competitive, multiplayer Real-Time Strategy (RTS) game inspired by the mechanical depth, responsiveness, and pacing of *StarCraft II*, using a distinctive **"Holographic Bubble & Dome" visual paradigm** instead of complex 3D skeletal character models.

### Core Pillars
1. **SC2-Grade Responsiveness & Depth:** Deterministic simulation, crisp stutter-step micro, high-ground vision rules, turn rates, and fluid group pathfinding.
2. **The Bubble & Dome Aesthetic:** Every mobile unit is a floating 3D energy/glass **sphere (bubble)** and every structure is a grounded **hemisphere (dome)** on a grid baseplate, each displaying looping video sequences (Idle, Walk, Attack, Construct, Produce) with hard cuts between states and expressive shader effects (trailing mist, rim indicators, progressive glass cracks, CRT glitching, and shattering on death).
3. **Multiplayer & Bot-First:** No single-player campaign or built-in scripted AI; instead, a first-class external **Bot API** allows third parties to write bots in any language (Python, Rust, C++) and run headless matches at high speed.
4. **Cross-Platform (Linux-First, Rust + Bevy):** Developed in Rust + Bevy (`wgpu`) on Linux with clean portability to Windows, macOS, and WebGPU/Wasm.

---

## 2. Visual Language of the Bubble

Because the bubble replaces traditional 3D unit meshes, its shader, frame, and particle effects must communicate **six critical pieces of combat information** at a glance—even when 100+ units are clumped together.

### 2.1 Answering the Question: Does "Pointing / Facing Direction" Matter in SC2?
**Yes—it is critical to SC2's combat feel and balance:**
- **Turn Rate & Firing Arc:** Most SC2 units (e.g., Marines, Marauders, Roaches, Stalkers) have a narrow weapon firing arc and a specific `TurnRate`. They *must* rotate to face their target before their attack wind-up (`DamagePoint`) can begin, and rotate again to retreat.
- **Turrets vs. Hull Facing:** Certain units (e.g., Siege Tanks, Immortals, Phoenixes, Battlecruisers) have **independent turrets** that track targets while the unit faces or moves in another direction.
- **Cone / Directional Abilities & Passives:** Abilities like Forcefields, Corrosive Bile, Liberator Defender Mode, or directional shields depend on orientation.

### 2.2 How the Bubble Communicates Each Attribute

| Information | Visual Mechanism | Details |
| :--- | :--- | :--- |
| **1. Movement Direction** | **Trailing Mist & Wake Particles** | Volumetric/ribbon particle trail streaming behind the bubble opposite to the velocity vector (as shown in the concept renders), plus a slight aerodynamic tilt of the ground ring. |
| **2. Facing / Pointing Direction** | **Equatorial Rim Chevron & Video Flip** | A glowing directional chevron/arrow hugging the lower rim of the bubble (visible in the concept art). Additionally, a 1-direction video can be horizontally mirrored (left/right) to match facing before 8-directional videos are introduced. |
| **3. Attack Direction** | **Focal Emitter Lens + Muzzle/Beam Tether** | **Proposal:** A bright "Aperture Ring / Focal Lens" slides along the surface of the bubble toward the target when attacking:<br>• **Hitscan (Marine-like):** Brief directional muzzle flash on the bubble's outer shell + fast tracer line + impact ripple on the target bubble.<br>• **Projectile (Stalker/Marauder-like):** Projectile spawns directly from the Focal Lens.<br>• **Beam/Channel (Void Ray/Sentry-like):** Continuous laser or lightning arc from the Focal Lens to the target bubble. |
| **4. Team Ownership** | **Ground Ring + Fresnel Rim Tint** | Crisp team-colored ground selection circle + colored outer rim glow (Fresnel shader) on the bubble sphere. Supports standard colors and Player/Ally/Enemy (Green/Yellow/Red) competitive colorblind mode. |
| **5. Unit Identity (Readability in Swarms)** | **Radius + Silhouette Frame + Altitude** | Video alone can blur together in 200-supply armies. Combine three cues:<br>• **Bubble Radius:** Scales with unit footprint (small = light infantry, medium = heavy infantry, large = massive/siege).<br>• **Rim Casing / Frame:** Distinct geometric collar/ornament on the bubble rim per unit class (e.g., spiked bracket for assault, heavy hex-frame for armored, halo ring for caster).<br>• **Hover Altitude & Shadow:** Air units float significantly higher with an offset ground shadow and tether indicator directly below them. |
| **6. Current State & Health/Damage** | **Hard-Cut Video + Shell Cracks & Glitch Shader** | • **Action:** Hard cut between looping video clips (`Standing`, `Walking`, `Shooting`, `Casting`).<br>• **Shields:** Outer bluish/team-tinted energy ripple when hit.<br>• **Hull Damage:** Progressive spiderweb cracks in the bubble glass + CRT scanlines/chromatic glitching on the inner video below 50% / 25% HP (as shown in `damaged bubbles` concept).<br>• **Death:** Bubble shatters into refractive shards with an explosion. |

### 2.3 Video Rendering Optimization (Crucial Technical Detail)
Decoding 400 simultaneous `.mp4` streams on the CPU/GPU will bottleneck any machine.
- **Solution:** Author animations as normal short video files (`.mp4`/`.webm`), and have an **Asset Pipeline Tool** automatically convert them into **GPU Texture Arrays / Compressed Flipbook Atlases (KTX2 / BC7)** at build time.
- At runtime, rendering 1,000 bubbles with independent animation frames becomes a **single GPU instanced draw call** where each bubble instance passes `(unit_type_id, anim_state, frame_index, health_pct, facing_angle, team_color)`.

---

## 3. Internal Simulation Engine: SC2 Capabilities & Data Model

To achieve SC2-level gameplay and deterministic multiplayer, the **Simulation Core** must be strictly separated from the **Rendering Engine** and run at a fixed tick rate (e.g., **22.4 ticks/sec** or **30 ticks/sec**) using deterministic math.

### 3.1 Unit State & Attributes to Track
1. **Identity & Classification**
   - `EntityId` (unique deterministic ID + generation counter)
   - `UnitTypeId`, `OwnerPlayerId`, `TeamId`
   - `Attributes` (bitflags): `Light`, `Armored`, `Biological`, `Mechanical`, `Psionic`, `Massive`, `Structure`, `Heroic`, `Summoned`
   - `SupplyCost` / `SupplyProvided`, `MineralCost`, `GasCost`, `BuildTime`
2. **Spatial & Movement**
   - `Position (x, y)`, `Altitude (z)`, `HeadingAngle`, `TurretAngle`
   - `Velocity (vx, vy)`, `MaxSpeed`, `Acceleration`, `Deceleration`, `TurnRate`
   - `CollisionRadius`, `SeparationRadius`, `PushPriority` (friendly moving units push lower-priority idle friendly units out of the way)
   - `MoverType`: `Ground`, `CliffWalker` (Colossus-style), `CliffJumper` (Reaper-style), `Air`
3. **Vitals & Defense**
   - `HP` (`current`, `max`, `regen_rate`)
   - `Shields` (`current`, `max`, `regen_rate`, `regen_delay_after_hit`)
   - `Energy` (`current`, `max`, `regen_rate`)
   - `HealthArmor`, `ShieldArmor` (flat damage reduction per hit, upgradable)
4. **Weapons (A unit can have 0, 1, or multiple weapons, e.g., Ground vs. Air)**
   - `TargetFilter` (e.g., `Ground`, `Air`, `Any`)
   - `BaseDamage`, `AttributeBonusDamage` (e.g., `+5 vs Armored`), `HitCount` (e.g., Zealot hits twice)
   - `Range`, `MinRange` (for siege units), `MaxTargetAngle` (firing arc)
   - `CooldownTicks` (attack period)
   - **`DamagePointTicks`** (wind-up time before damage/projectile occurs — enables stutter-stepping)
   - **`BackswingTicks`** (post-shot animation recovery that *can* be canceled by a Move command)
   - `DeliveryType`: `Instant/Hitscan`, `Projectile (speed, homing)`, `Beam/Persistent`
   - `Splash`: `None`, `Circular (100% / 50% / 25% radii)`, `Line/Cone`, `FriendlyFire (bool)`
5. **Vision, Stealth & Detection**
   - `SightRadius`
   - `Cloaked` / `Burrowed` flags (untargetable unless revealed)
   - `DetectorRadius` (reveals enemy cloaked/burrowed units)
6. **Orders, Abilities & Buffs**
   - `OrderQueue`: FIFO queue for Shift-queued commands (`Move`, `AttackMove`, `AttackTarget`, `Patrol`, `HoldPosition`, `Stop`, `UseAbility`, `Build`, `Harvest`, `ReturnCargo`)
   - `ActiveBuffs`: Timed status effects modifying speed, attack rate, armor, vision, stun, or applying damage-over-time (e.g., Stimpack, Concussive Shells, Fungal Growth).
   - `Cargo`: Transport capacity and loaded units (Medivac/Prism-style pickup & drop micro).

### 3.2 Map Capabilities (SC2 Parity)
- **Discrete Height / Cliff Levels (Levels 0, 1, 2, 3):**
  - Low-ground units **cannot see up a cliff** unless an allied unit or air unit provides high-ground vision.
  - Attacks from low ground to high ground require vision of the target.
- **Ramps:** Navigable slopes connecting cliff levels; configurable build-blocking rules (e.g., classic 3x3 wall-off at ramp top).
- **Dual Coordinate System:**
  - **1x1 Placement & Visibility Grid:** Used for building footprints, creep/territory spread, and Fog of War (`Unexplored`, `Explored/Fogged`, `Visible`).
  - **Continuous Sub-Grid Coordinates:** Used for smooth unit positions, radii, and steering.
- **Interactive Map Features:**
  - **Resources:** Mineral nodes (finite capacity, worker-pairing harvesting) and Gas Geysers (require constructing an extractor).
  - **Destructible / Collapsible Rocks:** Neutral structures with HP that block or collapse onto paths.
  - **Line-of-Sight (LoS) Blockers / Smoke Vents:** Obstruct ground vision until entered or viewed from the air.
  - **Watchtowers:** Neutral capture zones that grant large circular vision while held by a ground unit.
  - **Terrain Modifiers:** Faction territory (like Creep) or speed/slow zones.

---

## 4. Pathfinding & Unit Behavior Algorithms

SC2's acclaimed unit feel comes from a **three-layer movement architecture**:

1. **Global Pathfinding — Dynamic Constrained Delaunay Triangulation (CDT) NavMesh + Funnel Algorithm** (or Hierarchical Flow Fields):
   - Static cliffs and dynamically placed buildings carve polygon holes into a 2D Navigation Mesh.
   - `A*` searches across NavMesh triangles, and the **Simple Stupid Funnel Algorithm** pulls the path tight around corners so units move at arbitrary angles (not locked to 8 grid directions).
2. **Local Steering & Flocking — Separation + Reciprocal Velocity Obstacles (RVO / ORCA):**
   - Units moving in a group apply steering forces to avoid overlapping while maintaining formation velocity.
   - When attacking a target, units naturally slide around allies already in range to form a **concave firing arc** rather than getting stuck behind the front row.
3. **Push Priority & Collision Resolution:**
   - Moving friendly units with a command gently push idle friendly units (not on `Hold Position`) out of their way and then let them settle back.
   - Enemy units hard-block each other, enabling **body-blocking and surround micro**.

---

## 5. Networking Architecture: P2P vs. Client-Server

For an RTS with hundreds of units, syncing full entity positions over the network (like an FPS) requires heavy bandwidth and complex rollback/interpolation. Instead, RTS games use **Deterministic Lockstep**.

### Recommended Model: Deterministic Lockstep over a Lightweight Authoritative Relay Server
- **How it works:**
  1. Every client runs the exact same deterministic simulation.
  2. Clients only send **Player Inputs** (e.g., `Tick 1040: Select [u1, u2] -> AttackMove(x, y)`) to the server.
  3. The server collects inputs for tick `N`, stamps them, and broadcasts the tiny command packet (~50–200 bytes) to all players and spectators.
- **Why Relay Server beats pure P2P:**
  - **No NAT Traversal / Port Forwarding Headaches:** Players just connect outbound to the server.
  - **Security & Anti-Drop-Hack:** Hides player IP addresses (preventing DDoS) and clearly identifies which player disconnected if a connection drops.
  - **Instant Replays & Spectating:** Because a match is just `MapSeed + OrderedList<TickInputs>`, a full 30-minute match replay is only ~100 KB and spectators can watch live with minimal server load.
  - **Optional P2P/LAN Mode:** Since the server is just an input relay, any player can seamlessly host the relay locally for LAN/1v1 direct play.

---

## 6. Engine, Tech Stack & Map Editor Options

### 6.1 Engine Decision: Open Source Engine vs. Custom
Given **Linux-first development**, **cross-platform support (Windows, macOS, optional Web)**, and **deterministic simulation + custom bubble shaders**, **Rust** is an ideal language choice:

| Approach | Pros | Cons | Verdict |
| :--- | :--- | :--- | :--- |
| **A. Modular Rust (`sim_core` + Bevy / `wgpu`)** *(Recommended)* | • Pure, zero-dependency deterministic `sim_core` crate (can run headless at 10,000x speed for bots/servers).<br>• Bevy provides `wgpu` (Vulkan/Metal/DX12/WebGPU), WGSL custom shaders for bubbles, audio, UI, and input.<br>• Single language (Rust) across simulation, client, server, and map editor. | • Bevy's built-in ECS is geared toward general games; keep the deterministic RTS simulation in a dedicated fixed-tick module or deterministic ECS schedule. | **Best balance of control, performance, and speed of development.** |
| **B. Godot 4 + Rust (`gdext`)** | • Built-in visual editor, UI widgets, and particle tools out of the box. | • Splitting logic between GDScript/Godot scene tree and Rust deterministic simulation adds friction; harder to optimize 1,000 custom bubble video instances. | Good alternative if you prefer a GUI-heavy engine early on. |
| **C. From-Scratch Engine (`winit` + raw `wgpu`)** | • 100% control over every line of code. | • Months spent writing glTF loaders, UI layout, audio mixers, and asset pipelines before making the game. | Unnecessary wheel-reinvention when Bevy/libraries can handle rendering & I/O. |

### 6.2 Bot API Architecture (Third-Party AI)
Inspired by SC2's `s2client-proto`:
- The client or headless runner opens a local **WebSocket / Unix Socket / gRPC port** speaking **Protobuf or MessagePack/JSON**.
- **Per-Tick Observation:** Sends the bot its fog-of-war-filtered view (own units, visible enemy units, terrain/visibility grids, resources, upgrades).
- **Per-Tick Action:** Bot responds with a list of unit commands.
- Supports **Real-Time Mode** (human vs. bot) and **Step-Lock Headless Mode** (bot vs. bot running as fast as the CPU allows).

### 6.3 Map Editor Strategy
1. **Milestone 1 (Use Existing Tool — Tiled):** Author early maps in **Tiled** (open-source 2D tile/object editor) with layers for Cliff Level, Ramps, Resources, and Spawn Points. The engine reads the `.tmj` JSON file and procedurally generates the 3D terrain mesh, cliffs, and NavMesh.
2. **Milestone 2 (In-Engine 3D Map Editor):** Build an editor mode directly inside the game client using `egui`, allowing terrain height painting, ramp placement, decor/lighting tweaks, and instant one-click "Test Map" execution.

---

## 7. Phased Roadmap

- **Phase 1 — The Bubble Prototype & Visual Sandbox:**
  - Set up the Rust workspace (`sim_core`, `client`, `bot_api`, `server`).
  - Render the 3D map plane + Holographic Bubble shader (iridescent rim, video flipbook texture array, trailing mist particles, directional chevron, attack focal lens, crack/damage shader, shatter effect).
- **Phase 2 — Deterministic Core & Micro Feel:**
  - Fixed-tick simulation, selection boxes, control groups, `Move` / `AttackMove` / `Stop` / `HoldPosition`, `DamagePoint` + `Backswing` stutter-step micro, basic unit vs. unit combat.
- **Phase 3 — Pathfinding, Cliffs & Fog of War:**
  - CDT NavMesh + Funnel pathfinding, RVO/steering separation, push priority, cliff height levels, ramps, and high-ground Fog of War.
- **Phase 4 — Economy, Production & Full SC2 Mechanics:**
  - Minerals/gas harvesting, building placement grid, production queues, tech requirements, upgrades, shields/energy, cloaking/detection, abilities & buffs.
- **Phase 5 — Lockstep Multiplayer, Replays & Bot API:**
  - Relay server, deterministic lockstep networking, checksum desync detection, replay recorder/viewer, and external Bot API + Python starter kit.
- **Phase 6 — In-Engine Map Editor & Faction Polish:**
  - Integrated `egui` map editor, roster balancing, sound effects, HUD/minimap polish.
