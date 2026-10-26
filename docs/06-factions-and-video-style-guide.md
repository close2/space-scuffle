# 06 — Factions, Unit Roster & Video Art Style Guide

## 1. Visual & Video Art Direction (*StarCraft II* × *League of Legends* Stylized Aesthetic)

Every video displayed inside a unit **Bubble** (sphere) or building **Dome** (hemisphere)—as well as in the **Bottom-Center High-Resolution Selected Unit Portrait HUD**—must adhere to a unified art direction so the battlefield looks cohesive, vibrant, and readable at a glance.

### 1.1 Core Style Pillars
1. **Stylized Hand-Painted / Arcane-Grade 3D Rendering (Not Photorealistic):**
   - Bold, exaggerated proportions (chunky armor plates, oversized weapons, expressive silhouettes, glowing energy cores) inspired by *StarCraft II* cinematic portraits and *League of Legends* champion renders.
   - Painterly surface textures with clean gradients, sharp edge highlights, and saturated accent colors rather than noisy photorealistic grit.
2. **Dark Vignetted Radial Studio Backdrop (Inside the Bubble):**
   - **Rule:** The background inside every unit video must be a **deep, low-contrast dark slate/indigo radial vignette** (`#0B101B` to `#162238`) with subtle atmospheric floor rim-lighting.
   - **Why:** If the video background is bright or cluttered with scenery, the bubble looks like a flat square TV screen floating in the air. When the video background is dark and radially vignetted, the character pops inside the refractive glass sphere as if holographically projected inside the orb!
3. **Framing & Camera Angle:**
   - **3/4 Isometric-Front Hero Framing:** The character occupies **70%–80% of the vertical frame**, centered, viewed from a slight 20° elevated 3/4 angle looking toward the right side of the frame (our shader automatically mirrors horizontally when facing left).
   - **Dual-Purpose Resolution (`512×512` Master Flipbooks):**
     - All flipbooks are baked at **`512×512` resolution** so they look razor-sharp in the **Selected Unit Portrait HUD** (when a single unit is selected) while mipmapping cleanly inside the 3D battlefield bubbles.
4. **Action Readability (Hard Cuts):**
   - **`Idle`:** Calm breathing, subtle weapon glow pulse, radar/head scanning.
   - **`Walk`:** Dynamic forward lean, exaggerated stride/hover bobbing, dust/energy kick-up at feet.
   - **`Attack`:** High-contrast muzzle flash / blade slash / acid spit filling the right third of the frame with strong recoil and warm/bright lighting on the character.

---

## 2. The Three Factions & StarCraft II Mechanical Mapping

Instead of Terran, Protoss, and Zerg, **Space Scuffle** features three distinct, immediately recognizable sci-fi/nature archetypes whose biology and technology naturally explain their SC2-inspired gameplay mechanics:

### Faction 1: The Automaton Directorate ("Robots")
* **Visual Theme:** Sleek industrial war-machines, gunmetal & brushed cobalt-steel chassis, glowing cyan/amber optical visors, venting steam, hydraulic pistons, and magnetic railguns.
* **Mechanical Identity (Maps to Terran):**
  - Modular mechanical units that can be repaired by Welder Drones.
  - **Overclock (Stimpack):** Venting superheated reactor coolant for +50% movement and firing speed at the cost of hull integrity.
  - **Transformable Siege Modes & Lift-Off Domes:** Heavy artillery locks into the ground; production domes can fire thrusters to relocate.

| Space Scuffle Unit | SC2 Equivalent | Role & Capabilities | Visual Inside Bubble / Dome |
| :--- | :--- | :--- | :--- |
| **Welder Drone (`Worker`)** | SCV | Harvests minerals/gas, constructs domes, repairs mechanical hulls. | Compact hovering yellow/cobalt utility bot with twin plasma welding arcs and magnetic mineral clamp. |
| **Aegis Pulse Bot (`Marine`)** | Marine | All-purpose rapid-fire hitscan infantry (`Ground + Air`), **Overclock** ability. | Agile bipedal combat android in heavy cobalt carapace firing a rapid-cycle cyan pulse rifle. |
| **Shock Golem (`Marauder`)** | Marauder | Heavily armored anti-armor bruiser; fires gravity-concussion shells that slow targets. | Hulking broad-shouldered mech with dual shoulder-mounted concussion cannons launching glowing orange shells. |
| **Railgun Crawler (`SiegeTank`)** | Siege Tank | Quad-tracked artillery with **independent 360° rotating turret** and long-range splash damage. | Quad-legged/tracked artillery robot whose upper twin-railgun turret tracks and fires high-velocity shockwave rounds. |
| **Vector Interceptor (`SkyScout`)** | Viking / Medivac | Agile VTOL jet droid providing aerial superiority and high-ground spotting. | Sleek twin-thruster cybernetic jet fighter banking in mid-air with afterburners glowing. |
| **Foundry Core Dome (`CommandDome`)** | Command Center | `5x5` main base dome; produces Welder Drones and receives resources. | Massive robotic blast-furnace core with articulated gantry arms assembling droid frames. |
| **Assembly Bay Dome (`BarracksDome`)** | Barracks | `3x3` combat production dome; trains Aegis Pulse Bots and Shock Golems. | Automated robotic assembly line with laser welders and holographic blueprints. |
| **Capacitor Dome (`SupplyDome`)** | Supply Depot | `2x2` power grid dome; can retract into the floor to allow friendly passage. | Glowing high-voltage Tesla capacitor coils pulsing with cyan energy rings. |
| **Tesla Sentinel Dome (`LaserTurretDome`)** | Missile Turret | `2x2` defensive dome + **Detector**; fires continuous high-energy beams/arcs. | Rotating twin-coil electro-laser emitter tracking targets with crackling lightning arcs. |

---

### Faction 2: The Cosmic Enclave ("Classic Stereotypical Aliens / The Greys")
* **Visual Theme:** Classic retro-futuristic "Roswell Grey" extraterrestrials with large dark almond eyes and oversized craniums, dressed in iridescent chrome/violet exo-suits, wielding flying saucers, green tractor beams, psychic lightning, and antimatter disintegration rays.
* **Mechanical Identity (Maps to Protoss):**
  - Every unit and dome is protected by **regenerating Psionic Energy Shields** (which rapidly recharge after 10 seconds out of combat).
  - High-tech, high-cost precision specialists with teleportation (**Phase Blink**), energy blades, and warp-in dome technology.

| Space Scuffle Unit | SC2 Equivalent | Role & Capabilities | Visual Inside Bubble / Dome |
| :--- | :--- | :--- | :--- |
| **Abductor Saucer (`AlienWorker`)** | Probe | Harvests resources via green tractor beam; warps in alien domes. | Miniature spinning silver flying saucer projecting a bright green anti-gravity beam underneath. |
| **Psi-Blade Grey (`Zealot`)** | Zealot | Frontline alien warrior with high shields and **twin-hit (`2x`) plasma psi-blades**. | Classic big-headed Grey alien in sleek chrome/purple battle-armor lunging forward with twin glowing violet energy blades. |
| **Saucer Tripod (`Stalker`)** | Stalker / Immortal | Agile hovering alien walker firing antimatter bolts (`+Armored` bonus) with **Blink** teleport. | Classic three-legged hovering alien saucer-walker firing concentrated magenta/green disintegration beams. |
| **Cerebral Mind-Lord (`HighTemplar`)** | High Templar / Sentry | Telekinetic big-brain alien caster unleashing psychic storms and forcefields. | Floating large-cranium Grey alien mastermind in a levitating psi-throne surrounded by psychic lightning. |
| **Roswell Cruiser (`Phoenix`)** | Phoenix / Void Ray | Fast flying saucer that can fire prismatic beams while moving. | Iconic spinning chrome flying saucer with glowing emerald rim ports firing a continuous laser ray. |

---

### Faction 3: The Sylvan Swarm ("Animals, Insects & Weaponized Nature")
* **Visual Theme:** Ferocious, bioluminescent woodland predators, armored beetles, mantis hunters, and living flora—combining chitinous beetle carapaces, glowing emerald/amber venom sacs, antlers, and razor claws.
* **Mechanical Identity (Maps to Zerg):**
  - Passive **Organic Health Regeneration** on all units.
  - Fast swarm mobility, **Burrow / Camouflage** ambush mechanics, corrosive armor-melting acid, and living bio-domes.

| Space Scuffle Unit | SC2 Equivalent | Role & Capabilities | Visual Inside Bubble / Dome |
| :--- | :--- | :--- | :--- |
| **Forager Ant (`AnimalWorker`)** | Drone | Gathers minerals/sap and germinates living bio-domes. | Industrious giant iridescent leaf-cutter ant / beetle carrying glowing blue crystal shards in its mandibles. |
| **Razor Mantis (`Zergling`)** | Zergling | Ultra-fast melee swarm predator with rapid twin-scythe strikes. | Ferocious emerald-and-gold praying mantis / raptor hybrid lunging with glowing serrated forelimbs. |
| **Bombardier Rhino-Beetle (`Roach`)** | Roach / Hydralisk | Thickly armored beetle spraying pressurized corrosive bio-acid spines. | Heavy horned rhinoceros-beetle with glowing neon-green dorsal venom sacs firing bursts of acid. |
| **Stag Behemoth (`Ultralisk`)** | Ultralisk | Massive apex forest beast with cleaving antler/tusk shockwaves. | Colossal moss-and-chitin armored grizzly-stag behemoth charging with glowing amber antlers. |
| **Dragonfly Wasp (`Mutalisk`)** | Mutalisk | Fast aerial insect predator firing ricocheting venom stingers. | Iridescent four-winged giant hornet-dragonfly hovering rapidly and launching glowing stinger darts. |

---

## 3. Modular Bubble Visual Effects Architecture (Sub-Tasks Breakdown)

To achieve the exact visual quality of the concept renders while allowing iterative inspection, the bubble rendering pipeline is split into **6 independent visual subsystems**:

1. **V1 — Camera Framing, Asset Path Resolution & Automated Visual Tests:**
   - Workspace-aware asset path resolution (`assets/` resolved from any working directory) + close-up default camera framing in `--lab` mode + automated shader/texture/frustum integration tests.
2. **V2 — Bubble Glass Optics (Glare, Refraction & Fresnel Shell):**
   - Anisotropic top-left curved window glare highlight, inner depth darkening, chromatic soap-bubble thin-film iridescence on the rim, and spherical UV warp on the inner video plane so the video wraps convincingly inside a 3D glass orb.
3. **V3 — Selection Visuals & SC2-Style High-Res Selected Unit Portrait HUD:**
   - Left-click & box-drag selection in the 3D viewport, rotating tactical ground selection ring + overhead HP/Shield bar on selected entities, and a **Bottom-Center High-Res (`512×512`) Live Video Portrait & Vitals HUD** displaying the currently selected unit.
4. **V4 — Volumetric Trailing Mist, Wake Ribbons & Overclock Sparks:**
   - Multi-layered translucent vapor puffs, tiny trailing micro-bubbles, and high-speed orange/cyan spark streaks emitted opposite to the velocity vector.
5. **V5 — Rim Orientation & Combat Indicators (Curved Chevron & Attack Focal Lens):**
   - Glowing curved arrow hugging the lower front rim (Hull Facing) + independent equatorial Attack Aperture / Focal Lens emitting hitscan muzzle flashes, plasma bolts, red laser beams, and crackling lightning arcs.
6. **V6 — Diegetic Damage States (Spiderweb Glass Cracks, Smoke, CRT Glitch & Shatter):**
   - Progressive Voronoi glass fractures (`<75%` HP), CRT scanline tearing & RGB chromatic split (`<50%` HP), orange electrical arcing & black smoke plumes (`<35%` HP), and 3D glass shard shattering on death.
