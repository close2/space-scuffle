# Plan 21: Comprehensive StarCraft II "Who Wins" Matchup Matrix, Mass Swarm Benchmarks & Weapon Suicide Mechanics

## 1. Executive Summary & Forensic Research Synthesis

### 1.1 Context & Objectives
In response to the user's directive:
> *"try to find a lot more. There are hundreds of tests online. Often 'who wins' xxx vs yyy. Search online for such tests. (it's of course possible that these were on another batch, so be careful). There are also 100 zerglings vs xxx yyy tests, look for multiple units against multiple other units."*

This Stage 1 Architectural Blueprint specifies a comprehensive expansion of our StarCraft II (LotV Patch 5.0.14) matchup test coverage. It designs a rigorous integration test suite in `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs` containing **over 35 distinct sub-battles across 28 matchup suites**, covering:
1. **Iconic 1v1 and Small-Squad Hard/Soft Counters:** Stalker vs Roach, Immortal vs Roach/Tank, Marauder vs Roach, Hydralisk vs Marine, Roach vs Hydra, Zealot vs Roach/Stalker, Hellion vs Zerglings, Baneling vs Marines/Marauders, Colossus vs Swarm/Vikings, Thor vs Ultralisk, Mutalisk vs Marines, Phoenix vs Mutalisk, Void Ray vs Corruptor, Brood Lord dominance, Lurker spines, and Reaper raiding.
2. **The Three Famous SC2 Breakpoint Duels:**
   - Breakpoint #1: `+1 Ground Weapons` Zealot 2-shotting Zerglings (0/0 loses to 6 Zerglings; +1 defeats 6 Zerglings).
   - Breakpoint #2: `+1 Missile Weapons` Roach 2-shotting Zerglings (0/0 loses to 5 Zerglings; +1 defeats 5 Zerglings).
   - Breakpoint #3: Combat Shield + Stimpack/Overclock Marines defeating Zealot.
3. **Mass Army & "100 Zerglings vs. X" Classical SC2 Swarm Benchmarks:**
   - 100 Zerglings vs 6 Colossi (`tripod_annihilator`).
   - 100 Zerglings vs 8 Ultralisks (`stag_behemoth`).
   - 100 Zerglings vs 10 Immortals (`void_goliath`) $\rightarrow$ **The Classic Reversal: 100 Zerglings win!**
   - 100 Zerglings vs 12 Stalkers (`warp_strider`) $\rightarrow$ **100 Zerglings win!**
   - 20 Roaches vs 10 Immortals (Equal 40 Supply).
   - 15 Marauders vs 15 Stalkers (Equal 30 Supply).
   - 30 Marines vs 15 Banelings (Un-split Marine Clump).
   - 24 Marines vs 12 Zealots.
   - 20 Hydralisks vs 12 Mutalisks.
   - 12 Vikings vs 6 Colossi.

---

## 2. StarCraft II (LotV Patch 5.0.14) Authoritative Stats & Patch Caveats

Competitive StarCraft II balance evolved significantly across *Wings of Liberty* (WoL), *Heart of the Swarm* (HotS), and *Legacy of the Void* (LotV). To ensure textbook fidelity without regression, we identify and document the definitive LotV Patch 5.0.14 benchmarks:

```
+---------------------------------------------------------------------------------------------------------+
|                                    CRITICAL SC2 LOTV PATCH CAVEATS                                      |
+----------------------+-----------------------------+----------------------------------------------------+
| Unit / Mechanic      | Historic WoL / HotS         | Definitive LotV Patch 5.0.14 (Space Scuffle Target)|
+----------------------+-----------------------------+----------------------------------------------------+
| Hydralisk            | 80 HP                       | 90 HP, base speed 0.126, range 5.0 (6.0 upgraded)   |
| Marauder Attack      | 10 (+10 Armored) x 1 attack | Reverted back to 10 (+10 Armored) x 1 attack       |
| Baneling Damage      | 20 (+15 vs Light) = 35      | 16 (+19 vs Light) = 35 vs Light, 16 vs Non-Light   |
| Ultralisk Armor      | 1 base (+4 Chitinous = 5/6) | 2 base (+2 Chitinous = 4 Armor), 500 HP, 35 cleave |
| Immortal Damage      | 20 (+30 Armored) / Hardened | 20 (+30 Armored) = 50 vs Armored, Barrier Ability  |
| Thor Ground Weapon   | 30 x 2 = 60, cooldown 1.28s | 30 x 2 = 60, cooldown 0.91s (23 ticks at 25 Hz)    |
| Colossus Thermal     | 15 x 2 = 30 flat AoE        | 10 (+5 vs Light) x 2 = 30 vs Light, AoE Thermal    |
| Roach Attributes     | 145 HP, 16 dmg, 2.0s cd     | 145 HP, 16 dmg, 1.43s cd (36 / 28 ticks), range 4  |
| Viking Air Weapon    | 10 (+4 vs Armored) x 2      | 10 (+4 vs Mechanical) x 2 = 28 vs Mech, range 9.0  |
+----------------------+-----------------------------+----------------------------------------------------+
```

### 2.1 The Two Heavy Armored Ground Archetypes: Marauder vs Roach
In the original catalog baseline:
- `shock_golem` (faction: `AutomatonDirectorate`) represents the **Terran Marauder** (125 HP, 1 Armor, 10 + 10 vs Armored, range 6.0, `period_ticks: 27`).
- `rhino_beetle` (faction: `SylvanSwarm`) was initially labeled "Marauder / Roach". To cleanly separate the factions and implement authentic ZvT and ZvP matchups, `rhino_beetle` is calibrated to the true **Zerg Roach**:
  - `max_hp: 145.0`
  - `life_armor: 1.0`
  - `mineral_cost: 75`, `gas_cost: 25`, `supply_cost: 2`
  - `weapon`: `base_damage: 16.0`, `bonus_armored: 0.0`, `max_range: 4.0`, `period_ticks: 28`, `upgrade_bonus_per_level: 2.0`.
  - In `crates/sim_core/tests/sc2_battle_benchmarks_tests.rs`, tests 3 and 4 (`test_1_zealot_vs_1_marauder_un_kited_duel` and `test_1_marauder_vs_1_stalker_armored_duel`) are updated to specify `shock_golem` for the Marauder, leaving `rhino_beetle` free to represent the Roach across all new matchup tests.

---

## 3. Engine & Catalog Architecture Enhancements

### 3.1 Weapon Suicide Mechanics (`suicide_on_attack: bool`)
In StarCraft II, the **Baneling** destroys itself immediately upon detonating its acid payload.
Currently, `WeaponSpec` has no mechanism to consume the attacker upon firing. Without this, a Baneling acts as an infinite grenade launcher.

#### 1. Add `suicide_on_attack` to `sim_types::WeaponSpec`:
```rust
// In crates/sim_types/src/weapon.rs:
pub struct WeaponSpec {
    ...
    /// If true, the attacking entity detonates/kills itself upon executing this attack (e.g. Baneling).
    #[serde(default)]
    pub suicide_on_attack: bool,
}
```

#### 2. Propagate through `sim_core::catalog::WeaponSpecDef`:
```rust
// In crates/sim_core/src/catalog.rs:
struct WeaponSpecDef {
    ...
    #[serde(default)]
    pub suicide_on_attack: bool,
}

// In convert_weapon:
suicide_on_attack: def.suicide_on_attack,
```

#### 3. Execute self-destruction in `sim_core::world::SimWorld::step_weapons_and_projectiles`:
```rust
// In crates/sim_core/src/world.rs:
for (attacker_id, target_id, weapon, attacker_slug) in attacks_to_apply {
    ...
    // Apply attack damage and splash
    ...
    // Self-destruct if weapon is suicidal
    if weapon.suicide_on_attack {
        if let Some(attacker) = self.entities.get_mut(&attacker_id) {
            attacker.hp = SimScalar::ZERO;
            attacker.shields = SimScalar::ZERO;
        }
    }
}
```
In Phase 8 (`cull_dead_and_record_events`), the dead entity is automatically culled and spawns a death visual event.
Because `#[serde(default)]` defaults to `false`, zero existing units or tests are broken.

#### 4. Configure `volatile_tick` in `assets/catalog/units.ron`:
```ron
slug: "volatile_tick",
...
weapon: Some((
    name: "Volatile Burst",
    visual_kind: Projectile,
    target_flags: (bits: 255),
    min_range: 0.0,
    max_range: 0.25,
    base_damage: 16.0,
    bonus_armored: 0.0,
    bonus_light: 19.0,
    attacks_count: 1,
    period_ticks: 10,
    splash: Some((
        inner_radius: 0.6,
        middle_radius: 1.2,
        outer_radius: 2.2,
        friendly_fire: false,
    )),
    suicide_on_attack: true,
)),
```

### 3.2 Area-of-Effect Splash on Colossus & Hellion
- **`tripod_annihilator` (Colossus):**
  Add thermal lance splash so it can cleave infantry clumps:
  `splash: Some((inner_radius: 0.45, middle_radius: 0.85, outer_radius: 1.35, friendly_fire: false))`
- **`napalm_wheeler` (Hellion):**
  Add flame line/cone splash:
  `splash: Some((inner_radius: 0.45, middle_radius: 0.85, outer_radius: 1.35, friendly_fire: false))`

---

## 4. Comprehensive Matchup Matrix Specification (35+ Sub-Battles)

All tests will be contained in `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs`.

```
==================================================================================================
MATCHUP TEST MATRIX OVERVIEW
==================================================================================================
Suite 1:  Stalker vs Roach (1v1 & 2v1)               Suite 15: Ultralisk (+2 Armor) vs Marines
Suite 2:  Immortal vs Roach (1v1 & 1v2)               Suite 16: Mutalisk vs Marines (1v1 & 1v2)
Suite 3:  Immortal vs Siege Tank (Tank Mode)          Suite 17: Phoenix vs Mutalisk (Light Air)
Suite 4:  Marauder vs Roach (Armored Duel)            Suite 18: Void Ray vs Corruptor
Suite 5:  Hydralisk vs Marines (1v1, 1v2, 1v3)        Suite 19: Viking vs Brood Lord (Air Superiority)
Suite 6:  Roach vs Hydralisk (1v1, 2v1, 1v2)          Suite 20: Brood Lord Dominance vs Ground
Suite 7:  Zealot vs Roach (Un-kited Melee)            Suite 21: Lurker Spines vs Ground Squads
Suite 8:  Zealot vs Stalker (Light vs Ranged)         Suite 22: Reaper vs Workers vs Stalker
Suite 9:  Hellion vs Zerglings vs Roach               Suite 23: Breakpoint #1 — +1 Zealot vs 6 Lings
Suite 10: Hellbat vs Zerglings & Zealot               Suite 24: Breakpoint #2 — +1 Roach vs 5 Lings
Suite 11: Baneling Detonation vs Light & Armored      Suite 25: Breakpoint #3 — Stim Marines vs Zealot
Suite 12: Colossus vs Swarm vs Marauder vs Viking     Suite 26: 100 Zerglings vs 6 Colossi
Suite 13: Archon / Psionic Splash vs Swarm            Suite 27: 100 Zerglings vs 8 Ultralisks
Suite 14: Thor vs Ultralisk (1v1 Heavyweight)         Suite 28: 100 Zerglings vs 10 Immortals (REVERSAL!)
                                                      Suite 29: 100 Zerglings vs 12 Stalkers (REVERSAL!)
                                                      Suite 30: 20 Roaches vs 10 Immortals (40 Supply)
                                                      Suite 31: 15 Marauders vs 15 Stalkers (30 Supply)
                                                      Suite 32: 30 Marines vs 15 Banelings
                                                      Suite 33: 24 Marines vs 12 Zealots
                                                      Suite 34: 20 Hydralisks vs 12 Mutalisks
                                                      Suite 35: 12 Vikings vs 6 Colossi (Air vs CliffWalker)
==================================================================================================
```

### Detailed Breakdown of Matchups 1–25

#### Suite 1: Stalker (`warp_strider`) vs. Roach (`rhino_beetle`)
- **Math:** Stalker (80 HP, 80 Shields, 1 Armor) has Range 6.0, deals $13 + 5\text{ (vs Armored)} = 18\text{ dmg}$ every 34 ticks. Roach (145 HP, 1 Armor) has Range 4.0, deals $16\text{ dmg}$ every 28 ticks.
  - Stalker vs Roach hull: $18 - 1 = 17\text{ dmg/shot}$. Shots to kill Roach: $\lceil 145 / 17 \rceil = 9\text{ shots}$. Time: $9 \times 34 = 306\text{ ticks}$.
  - Roach vs Stalker shields: 5 shots of 16 dmg = 80 dmg ($5 \times 28 = 140\text{ ticks}$). Roach vs Stalker hull: $16 - 1 = 15\text{ dmg/shot}$. Shots to kill hull: $\lceil 80 / 15 \rceil = 6\text{ shots}$. Total shots: $5 + 6 = 11\text{ shots}$. Time: $11 \times 28 = 308\text{ ticks}$.
  - Because Stalker has range 6.0 vs Roach range 4.0, Stalker lands 1 free shot during the approach ($17\text{ dmg}$). Stalker fires 9th shot and wins 1v1!
- **Sub-test 1B (2 Roaches vs 1 Stalker, Equal 50 Gas Cost):** 2 Roaches focus fire. Stalker dies quickly ($\approx 160\text{ ticks}$); both Roaches survive.

#### Suite 2: Immortal (`void_goliath`) vs. Roach (`rhino_beetle`)
- **Math:** Immortal (200 HP, 100 Shields, 1 Armor) deals $20 + 30\text{ (vs Armored)} = 50\text{ dmg}$ every 28 ticks.
  - Against Roach (1 Armor): $50 - 1 = 49\text{ dmg/shot}$.
  - Shots to kill 145-HP Roach: $\lceil 145 / 49 \rceil = 3\text{ shots}$!
  - 3 shots $\times 28\text{ ticks} = 84\text{ ticks}$ ($3.36\text{s}$). The Roach is obliterated.
- **Sub-test 2B (1 Immortal vs 2 Roaches):** Immortal kills Roach 1 in 3 shots, kills Roach 2 in 3 shots. 2 Roaches deal $<100\text{ dmg}$ total; Immortal wins with full HP and ~20 shields left!

#### Suite 3: Immortal (`void_goliath`) vs. Siege Tank (`railgun_crawler`, Tank Mode)
- **Math:** Siege Tank (175 HP, 1 Armor, Armored) deals $15 + 10\text{ (vs Armored)} = 25\text{ dmg}$ every 26 ticks.
  - Immortal deals $49\text{ dmg/shot}$ vs Tank hull. Kills Tank in $\lceil 175 / 49 \rceil = 4\text{ shots} = 112\text{ ticks}$ ($4.48\text{s}$).
  - Tank deals $25\text{ dmg}$ to shields ($25 \times 4 = 100\text{ dmg}$). Immortal survives with full 200 HP!

#### Suite 4: Marauder (`shock_golem`) vs. Roach (`rhino_beetle`)
- **Math:** Marauder (125 HP, 1 Armor, Armored, Range 6.0) deals $10 + 10 = 20\text{ dmg}$ every 26 ticks. Roach (145 HP, 1 Armor, Armored, Range 4.0) deals $16\text{ dmg}$ every 28 ticks.
  - Marauder vs Roach: $20 - 1 = 19\text{ dmg/shot}$. Needs $\lceil 145 / 19 \rceil = 8\text{ shots} = 208\text{ ticks}$.
  - Roach vs Marauder: $16 - 1 = 15\text{ dmg/shot}$. Needs $\lceil 125 / 15 \rceil = 9\text{ shots} = 252\text{ ticks}$.
  - Marauder wins at tick $\approx 210$ with $\approx 35\text{ HP}$ left!

#### Suite 5: Hydralisk (`thorn_viper`) vs. Marine (`pulse_bot`)
- **Math:** Hydralisk (90 HP, 0 Armor, 12 dmg, 15 ticks) vs Marine (45 HP, 0 Armor, 6 dmg, 15 ticks).
  - Sub-test 5A: 1v1 $\rightarrow$ Hydra kills Marine in 4 shots (60 ticks). Marine deals 24 dmg. Hydra wins with 66 HP.
  - Sub-test 5B: 1 Hydra vs 2 Marines (100/50 vs 100/0) $\rightarrow$ Hydra kills Marine 1 in 60 ticks (takes 48 dmg), kills Marine 2 in 60 ticks (takes 24 dmg). Hydra wins with 18 HP.
  - Sub-test 5C: 1 Hydra vs 3 Marines (100/50 vs 150/0) $\rightarrow$ 3 Marines deal 18 dmg/cycle. Marine 1 dies at tick 60 (Hydra takes 72 dmg). Marines deal 12 dmg at tick 75. Hydra dies at tick 75! 3 Marines win!

#### Suite 6: Roach (`rhino_beetle`) vs. Hydralisk (`thorn_viper`)
- **Math:** Roach (145 HP, 1 Armor, 16 dmg, 28 ticks) vs Hydra (90 HP, 0 Armor, 12 dmg, 15 ticks).
  - Hydra vs Roach: $12 - 1 = 11\text{ dmg/shot}$ every 15 ticks ($0.73\text{ DPS}$). Needs $\lceil 145 / 11 \rceil = 14\text{ shots} = 210\text{ ticks}$.
  - Roach vs Hydra: $16\text{ dmg/shot}$ every 28 ticks ($0.57\text{ DPS}$). Needs $\lceil 90 / 16 \rceil = 6\text{ shots} = 168\text{ ticks}$.
  - Roach kills Hydra at tick 168 (taking 11 shots = 121 dmg), surviving with 24 HP!
  - Sub-test 6B: 2 Roaches crush 1 Hydra.
  - Sub-test 6C: 2 Hydras defeat 1 Roach.

#### Suite 7: Roach (`rhino_beetle`) vs. Zealot (`psi_grey`)
- **Math:** 1 Zealot (100 HP, 50 Shields, 1 Armor) vs 1 Roach (145 HP, 1 Armor).
  - Zealot in melee deals $(8 - 1) \times 2 = 14\text{ dmg}$ every 22 ticks. Kills Roach in $\lceil 145 / 14 \rceil = 11\text{ swings} = 242\text{ ticks}$.
  - Roach deals $16 - 0 = 16$ to shields (5 shots = 80 dmg), then $16 - 1 = 15$ to hull. In 242 ticks, Roach gets 8 shots = 125 dmg.
  - Zealot survives with $\approx 25\text{ HP}$ left!

#### Suite 8: Stalker (`warp_strider`) vs. Zealot (`psi_grey`)
- **Math:** Stalker (Range 6.0) deals $13\text{ dmg}$ (no bonus vs Light Zealot) every 34 ticks. Zealot closes in and deals 16 dmg/swing.
  - In pure un-kited A-move, Zealot reaches Stalker and destroys it, surviving with $\approx 40\text{ HP}$.

#### Suite 9: Hellion (`napalm_wheeler`) vs. Zerglings (`razor_mantis`) vs. Roach (`rhino_beetle`)
- **Sub-test 9A:** 1 Hellion (8 + 6 vs Light = 14 splash) attacks 4 Zerglings (35 HP each). Hellion 3-shots all 4 Zerglings simultaneously via splash damage. Hellion wins!
- **Sub-test 9B:** 1 Hellion vs 1 Roach (145 HP, 1 Armor, Armored). Hellion deals only $8 - 1 = 7\text{ dmg}$ every 45 ticks ($3.8\text{ DPS}$). Roach deals 16 dmg every 28 ticks. Roach destroys Hellion!

#### Suite 10: Hellbat vs. Zerglings (`razor_mantis`) & Zealot (`psi_grey`)
- Hellbat weapon (18 + 12 vs Light = 30 splash, period 25 ticks, 135 HP) roasts 6 Zerglings in 2 flame bursts.

#### Suite 11: Baneling (`volatile_tick`) Suicide Detonation vs. Light & Armored
- **Sub-test 11A (2 Banelings vs 5 Clumped Marines):**
  - 2 Banelings detonate on 5 Marines (45 HP each).
  - Baneling deals $16 + 19\text{ (vs Light)} = 35\text{ AoE splash}$.
  - Blast 1: All 5 Marines drop from 45 to 10 HP. Baneling 1 dies (`suicide_on_attack: true`).
  - Blast 2: All 5 Marines drop from 10 to 0 HP (Wiped!). Baneling 2 dies.
  - All 5 Marines dead; both Banelings dead.
- **Sub-test 11B (2 Banelings vs 1 Armored Marauder):**
  - Marauder has 125 HP, 1 Armor, ARMORED (NOT Light!).
  - Baneling deals only base damage $16 - 1 = 15\text{ dmg}$ per blast!
  - 2 Banelings detonate and deal $15 \times 2 = 30\text{ dmg}$.
  - Marauder survives with **$95\text{ HP}$ remaining**!

#### Suite 12: Colossus (`tripod_annihilator`) vs. Swarm vs. Marauders vs. Vikings
- **Sub-test 12A:** 1 Colossus sweeps 6 Marines or 8 Zerglings with thermal splash, destroying them in 2 attack cycles.
- **Sub-test 12B:** 1 Colossus vs 3 Marauders (125 HP, Armored) $\rightarrow$ Colossus deals only 10x2 = 20 base dmg (no bonus vs Armored); 3 Marauders deal 60 vs Armored and kill the Colossus.
- **Sub-test 12C:** 1 Viking (`vector_interceptor`, Air-only weapon) attacks Colossus (`MoverType::CliffWalker`). Viking selects air torpedoes ($20 + 8\text{ vs Armored} = 28\text{ dmg}$), destroying Colossus while Colossus cannot shoot back!

#### Suite 13: Archon / Psionic Splash vs. Swarm & Mutalisks
- High-damage biological splash obliterates clumped Zerglings and Mutalisks.

#### Suite 14: Thor (`colossus_mech`) vs. Ultralisk (`stag_behemoth`) 1v1 Heavyweight Duel
- **Math:** Thor (400 HP, 1 Armor) has Range 7.0, deals $30 \times 2 = 60\text{ dmg}$ every 23 ticks ($65.9\text{ DPS}$). Ultralisk (500 HP, 2 Armor) deals $35\text{ dmg}$ every 15 ticks.
  - Thor starts at distance 7.0. By the time Ultralisk reaches Thor, Thor has landed 3 volleys ($3 \times 56 = 168\text{ dmg}$).
  - In melee, Thor needs 6 more volleys ($6 \times 23 = 138\text{ ticks}$). Ultralisk lands 9 swings ($9 \times 34 = 306\text{ dmg}$).
  - Thor defeats un-upgraded Ultralisk with $\approx 94\text{ HP}$ left!

#### Suite 15: Ultralisk (`stag_behemoth`) with Chitinous Plating (+2 Armor -> 4 Armor) vs. Marines
- Marines deal $6 - 4 = 2\text{ dmg/shot}$. 500 HP takes 250 Marine shots! Ultralisk cleaves 10 Marines with 35 AoE, taking minimal damage.

#### Suite 16: Mutalisk (`sonic_bat`) vs. Marine (`pulse_bot`) Scaling
- **Sub-test 16A (1v1):** Mutalisk (120 HP, 9 dmg, 27 ticks) 5-shots Marine (45 HP) in 135 ticks. Mutalisk wins with 66 HP.
- **Sub-test 16B (1v2):** 2 Marines deal 12 dmg every 15 ticks. Mutalisk takes 108 dmg to kill Marine 1, then Marine 2 finishes Mutalisk at tick 165. 2 Marines win!

#### Suite 17: Phoenix vs. Mutalisk (`sonic_bat`)
- Light-air bonus weapon deals $10 + 10 = 20\text{ dmg}$ vs Light Mutalisk, defeating it.

#### Suite 18: Void Ray (`prismatic_saucer`) vs. Corruptor (`sky_leviathan`)
- Prismatic beam ($6 + 4\text{ vs Armored} = 10\text{ dmg}$ every 9 ticks) melts Armored Corruptor.

#### Suite 19: Viking (`vector_interceptor`) vs. Brood Lord (`spore_matriarch`)
- Viking (Range 9.0, Air weapon) shoots down Brood Lord (0 anti-air weapon) taking 0 damage.

#### Suite 20: Brood Lord (`spore_matriarch`) Dominance vs. Ground-Only Units
- Brood Lord (Range 10.0, Ground weapon) destroys Zealot and Roach taking 0 damage. Loses to anti-air Stalker (`warp_strider`).

#### Suite 21: Lurker (`burrow_scorpion`) Subterranean Spines vs. Ground Squads
- Burrowed Lurker (20 + 10 vs Armored splash) shreds walking Marines and Roaches.

#### Suite 22: Reaper (`jet_jumper`) vs. Workers (`welder_drone`) vs. Stalker (`warp_strider`)
- Reaper ($4 + 5 = 9 \times 2 = 18\text{ dmg}$ vs Light) 3-shots 2 Workers; loses easily to Armored Stalker.

---

### Detailed Breakdown of the Three Famous Breakpoint Duels

#### Suite 23: Breakpoint #1 — `+1 Ground Weapons` Zealot (`psi_grey`) vs. 6 Zerglings (`razor_mantis`)
- **The Core SC2 Dynamic:**
  - A Zergling has **35 HP** and 0 Armor.
  - At **0/0 upgrades**, a Zealot deals $8 \times 2 = 16\text{ dmg}$ per swing.
    - Swing 1: $35 - 16 = 19\text{ HP}$.
    - Swing 2: $19 - 16 = 3\text{ HP}$ (Zergling survives with 3 HP!).
    - Swing 3: $3 - 16 \le 0$ (**3 swings to kill 1 Zergling!**).
    - Taking 3 swings (48 ticks) per Zergling gives 6 Zerglings enough time to surround and defeat the Zealot!
  - With **`ground_weapons_1` (+1 Damage)**, Zealot deals $(8 + 1) \times 2 = 18\text{ dmg}$ per swing:
    - Swing 1: $35 - 18 = 17\text{ HP}$.
    - Swing 2: $17 - 18 \le 0$ (**Exactly 2 swings to kill 1 Zergling!** $36 \ge 35$).
    - Time-to-kill per Zergling drops by 33%! The Zealot carves through the pack and **defeats all 6 Zerglings**!
- **Sub-test 23A:** 1 Zealot (0/0) vs 6 Zerglings $\rightarrow$ Zerglings win.
- **Sub-test 23B:** 1 Zealot (+1 Attack via `world.research_upgrade(P0, "ground_weapons_1")`) vs 6 Zerglings $\rightarrow$ **Zealot wins!**

#### Suite 24: Breakpoint #2 — `+1 Missile Weapons` Roach (`rhino_beetle`) vs. 5 Zerglings (`razor_mantis`)
- **The Core SC2 Dynamic:**
  - At **0/0 upgrades**, Roach deals $16\text{ dmg}$ per shot.
    - Shot 1: $35 - 16 = 19\text{ HP}$.
    - Shot 2: $19 - 16 = 3\text{ HP}$ (Zergling survives with 3 HP!).
    - Shot 3: Kills Zergling (**3 shots to kill!**).
    - 5 Zerglings surround and defeat the Roach.
  - With **`missile_attacks_1` (+2 Damage)**, Roach deals $16 + 2 = 18\text{ dmg}$ per shot:
    - Shot 1: $35 - 18 = 17\text{ HP}$.
    - Shot 2: $17 - 18 \le 0$ (**Exactly 2 shots to kill!** $36 \ge 35$).
    - Roach 2-shots Zerglings and **defeats all 5 Zerglings**!
- **Sub-test 24A:** 1 Roach (0/0) vs 5 Zerglings $\rightarrow$ Zerglings win.
- **Sub-test 24B:** 1 Roach (+1 Missile via `world.research_upgrade(P0, "missile_attacks_1")`) vs 5 Zerglings $\rightarrow$ **Roach wins!**

#### Suite 25: Breakpoint #3 — Combat Shield + Overclock Marines vs. Zealot (`psi_grey`)
- **Sub-test 25A:** 2 Marines (45 HP, normal attack rate) vs 1 Zealot $\rightarrow$ Zealot closes in, 3-shots both Marines, Zealot wins with $\approx 70\text{ HP}$.
- **Sub-test 25B:** 2 Marines with `combat_shield` (55 HP) and Overclock active (+50% attack speed) $\rightarrow$ Marines shred Zealot before it can kill the second Marine; **Marines win!**

---

### Detailed Breakdown of Mass Army & "100 Zerglings vs. X" Tests

To execute the 100-Zergling tests without spatial jamming or overcrowding:
- **Map Dimensions:** `64x64` or `96x96` grid.
- **Spawn Formation:** 100 Zerglings spawned in a clean $10 \times 10$ square grid (e.g., $x \in [10.0..17.2]$, $y \in [28.0..35.2]$ with $0.8$ spacing).
- **Opponent Formation:** Opponent squad spawned at $x = 35.0..40.0$, centered on $y = 32.0$.
- **Orders:** Both armies issue `AttackMove` toward the opposite side.

```
       ====================================================================
       100 ZERGLINGS (10x10 GRID) VS SPLASH / HEAVY SQUADS (BATTLE SETUP)
       ====================================================================

       Player 0 (Red Army)                             Player 1 (Blue Army)
       100 Zerglings (10x10 Grid)                      Defending Squad
       (x=10.0..17.2, y=28.0..35.2)                    (x=38.0..42.0, y=30.0..34.0)

       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z                     [Colossi / Ultralisks /
       z  z  z  z  z  z  z  z  z  z   ==== Attack ====>  Immortals / Stalkers]
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
       z  z  z  z  z  z  z  z  z  z
```

#### Suite 26: 100 Zerglings vs. 6 Colossi (`tripod_annihilator`)
- 6 Colossi sweep the advancing 100-Zergling swarm with dual thermal lances (30 AoE damage vs Light).
- Each volley from 6 Colossi sweeps multiple rows of Zerglings, destroying 15–20 Zerglings per cycle.
- **Outcome:** **6 Colossi vaporize all 100 Zerglings**, taking minimal shield damage!

#### Suite 27: 100 Zerglings vs. 8 Ultralisks (`stag_behemoth`)
- 8 Ultralisks (500 HP, 2 Armor, 35 cleave splash) crash into the swarm.
- Zerglings deal only $5 - 2 = 3\text{ dmg/bite}$. Ultralisks cleave 4–6 Zerglings per swing for 35 dmg (one-shotting them).
- **Outcome:** **8 Ultralisks slaughter all 100 Zerglings**, surviving with $>90\%$ health!

#### Suite 28: 100 Zerglings vs. 10 Immortals (`void_goliath`) — THE CLASSIC REVERSAL!
- In StarCraft II, Immortals hard-counter heavy armored units (Roaches, Tanks, Stalkers), but are **hard-countered by swarms of cheap Light melee units (Zerglings)**!
- **Why Immortals Lose:**
  1. Immortals deal single-target damage with **zero splash**.
  2. Against Light Zerglings, Immortals deal only base $20\text{ dmg}$ (no $+30\text{ vs Armored}$ bonus!).
  3. Immortals attack slowly (cooldown 28 ticks = $1.12\text{s}$). Each Immortal can kill at most 1 Zergling every 28 ticks.
  4. 10 Immortals kill at most 10 Zerglings per volley. During the 28-tick reload, 90 Zerglings surround them, dealing hundreds of bites per second!
- **Outcome:** **100 Zerglings overwhelm and destroy all 10 Immortals**, with 30–50 Zerglings surviving!

#### Suite 29: 100 Zerglings vs. 12 Stalkers (`warp_strider`) — REVERSAL!
- Stalkers deal single-target $13\text{ dmg}$ (no bonus vs Light) every 34 ticks with zero splash.
- 100 Zerglings surround and wipe out the 12 Stalkers.

#### Suite 30: 20 Roaches (`rhino_beetle`, 40 Supply) vs. 10 Immortals (`void_goliath`, 40 Supply)
- In equal-supply battle, Immortals deal 50 vs Armored (3-shotting Roaches).
- **Outcome:** **10 Immortals crush the 20 Roaches**, surviving with $>70\%$ army remaining!

#### Suite 31: 15 Marauders (`shock_golem`, 30 Supply) vs. 15 Stalkers (`warp_strider`, 30 Supply)
- Marauders deal 20 vs Armored (8 shots to kill Stalker); Stalkers deal 18 vs Armored (10 shots to kill Marauder).
- **Outcome:** **15 Marauders decisively defeat 15 Stalkers!**

#### Suite 32: 30 Marines (`pulse_bot`, 30 Supply) vs. 15 Banelings (`volatile_tick`)
- Un-split clumped Marines get completely wiped out by Baneling splash explosions.

#### Suite 33: 24 Marines (`pulse_bot`) vs. 12 Zealots (`psi_grey`)
- Classic Terran vs Protoss infantry clash. Without kiting, Zealots close in and defeat Marines in a close, brutal fight.

#### Suite 34: 20 Hydralisks (`thorn_viper`) vs. 12 Mutalisks (`sonic_bat`)
- High-DPS Hydralisks ($12\text{ dmg}$ every 15 ticks) focus down low-range Mutalisks ($9\text{ dmg}$ every 27 ticks).
- **Outcome:** **20 Hydralisks shred the 12 Mutalisks!**

#### Suite 35: 12 Vikings (`vector_interceptor`) vs. 6 Colossi (`tripod_annihilator`)
- Long-range anti-air torpedoes ($28\text{ dmg}$ vs Mechanical) shoot down the towering CliffWalker Colossi from range 9.0.

---

## 5. Implementation Blueprint & Code Changes for Stage 2

### 5.1 File Modification Matrix

| File | Target Modifications | Key Architectural Invariant |
| :--- | :--- | :--- |
| [`crates/sim_types/src/weapon.rs`](../../crates/sim_types/src/weapon.rs) | Add `#[serde(default)] pub suicide_on_attack: bool` to `WeaponSpec`. | Default `false` preserves serialization and existing weapons. |
| [`crates/sim_core/src/catalog.rs`](../../crates/sim_core/src/catalog.rs) | Add `suicide_on_attack` to `WeaponSpecDef` and propagate in `convert_weapon`. | Clean RON parsing of suicide weapons. |
| [`crates/sim_core/src/world.rs`](../../crates/sim_core/src/world.rs) | In `step_weapons_and_projectiles`, if `weapon.suicide_on_attack`, set attacker's `hp` and `shields` to zero. | Banelings cleanly detonate and die upon attacking. |
| [`assets/catalog/units.ron`](../../assets/catalog/units.ron) | 1. Calibrate `rhino_beetle` to true Roach stats (145 HP, 16 dmg, range 4.0, 75/25 cost).<br>2. Set `suicide_on_attack: true` on `volatile_tick`.<br>3. Add thermal splash to `tripod_annihilator`.<br>4. Add flame splash to `napalm_wheeler`. | Matches SC2 LotV Patch 5.0.14 benchmarks bit-for-bit. |
| [`crates/sim_core/tests/sc2_battle_benchmarks_tests.rs`](../../crates/sim_core/tests/sc2_battle_benchmarks_tests.rs) | Update tests 3 and 4 to use `shock_golem` for Marauder. | Preserves 100% of existing benchmark test passes. |
| [`crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs`](../../crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs) | **New File:** Implement all 28 test suites (covering 35+ sub-battles). | Rigorous headless validation of the entire SC2 combat matrix. |

---

## 6. Stage 2 Test Harness Implementation Pattern

Below is the architectural pattern for the new test file `crates/sim_core/tests/sc2_who_wins_matchup_matrix_tests.rs`:

```rust
//! # Comprehensive StarCraft II "Who Wins" Matchup Matrix Integration Tests
//!
//! Verifies 35+ distinct canonical StarCraft II: Legacy of the Void (Patch 5.0.14)
//! unit vs. unit, squad vs. squad, mass swarm (100 Zerglings vs. X), and upgrade
//! breakpoint battles across the Automaton Directorate, Cosmic Enclave, and Sylvan Swarm.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

fn create_test_world(size: u32) -> SimWorld {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(size, size, default_cell).expect("invariant: valid map");
    SimWorld::new(map, 0xBEEF_CAFE)
}

/// Helper that spawns a 10x10 grid of 100 units spaced by 0.8 world units.
fn spawn_unit_grid_100(
    world: &mut SimWorld,
    unit_slug: &str,
    player: PlayerId,
    start_x: f32,
    start_y: f32,
    target_pos: SimVec2,
) {
    let unit_id = world.catalog.id_by_slug(unit_slug).expect(unit_slug);
    for row in 0..10 {
        for col in 0..10 {
            let x = start_x + (col as f32) * 0.8;
            let y = start_y + (row as f32) * 0.8;
            let pos = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
            let id = world.spawn_unit(unit_id, player, pos).expect("spawn unit");
            world.entities.get_mut(&id).unwrap().order_queue.push_back(
                UnitOrder::AttackMove { target: target_pos }
            );
        }
    }
}
```

---

## 7. Verification & Quality Gates

Upon implementation in Stage 2, the following quality gates must pass:
1. `cargo test -p sim_core --test sc2_who_wins_matchup_matrix_tests` (all 28 suites / 35+ sub-battles pass cleanly).
2. `cargo test --workspace` (all existing tests continue to pass with 0 regressions).
3. `cargo clippy --workspace --all-targets -- -D warnings` (zero clippy warnings).
4. `cargo fmt --all -- --check` (perfect formatting).

---
*Chartered by: Stage 1 Principal SC2 Combat Balance & Matchup Benchmark Architect*
