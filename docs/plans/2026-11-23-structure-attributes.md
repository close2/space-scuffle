# Blueprint: Structure Attributes as Data (B-88)

This plan settles bug B-88 of the register against StarCraft II 5.0.14:

- **Structure attributes become authored data.** Every row of `assets/catalog/structures.ron` names its
  attributes, as every unit row already does. The conversion stops deriving them from the shields
  (`Psionic` if shielded, else `Mechanical`), which today gives Protoss structures an extra `Psionic` and Zerg
  structures `Mechanical` instead of `Biological`. The game's sets, read in the 5.0.14 data for all 49 structures
  of the catalog: Terran **Armored, Mechanical, Structure**; Protoss **Armored, Structure**; Zerg **Armored,
  Biological, Structure** (§2).
- **`Structure` means "a structure row", and validation guarantees it.** A structure row must name `Structure`;
  a unit row must not. So the two engine predicates for "is a structure" (the attribute, read by combat; the
  building shape, read by effect filters) agree for every catalog that loads (D-3).

A full plan is required (charter §6): a new required catalog field, a validation rule on two kinds of rows, a
change of rule in the catalog conversion, and work across the catalog, `sim_types`, `sim_core` and the records.
The change is **behaviour-neutral in every world that can be built today**: no shipped weapon, effect or
ability reads `Biological`, `Mechanical` or `Psionic` on a ground structure (§3.2), and no attribute is hashed
(§5). It removes a latent defect that the first such consumer (the Archon's bonus, the Viking's landed weapon,
Repair, Transfusion) would turn into a gameplay bug.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`d2671045` (checksum schema 35).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: RON data files instead of the editor's data (S-1) | Sanctioned | D-1 (the attribute names are the game's; the encoding is ours) |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6 |
| Owner question | **None needed.** Every one of the 49 rows is sourced [DATA]; nothing is left to the owner's judgement | — |

---

## 1. Summary

| | Today (`d2671045`) | After this plan |
|---|---|---|
| Where a structure's attributes come from | Derived in `StructureArchetypeDef::into_archetype`: `Structure \| Armored`, plus `Psionic` if `max_shields > 0`, else `Mechanical` | The row's authored `attributes: [..]` list, converted as written |
| Terran structures (17) | Armored, Mechanical, Structure (bits 74) — right by coincidence | Unchanged (74), now authored |
| Protoss structures (15) | Armored, **Psionic**, Structure (82) | Armored, Structure (**66**) |
| Zerg structures (17) | Armored, **Mechanical**, Structure (74) | Armored, **Biological**, Structure (**70**) |
| Validation of attributes | None for structures (nothing authored); none for units (`from_bits_truncate` drops unknown bits silently; an empty set is accepted) | Structures: required, non-empty, no duplicate, must name `Structure`. Units: non-empty, no unknown bit, must not name `Structure` |
| "Is a structure" | Two predicates (attribute flag in combat, building shape in effect filters) that agree only because the derivation always adds `Structure` | The same two predicates, kept in agreement by validation and pinned by a property test over the shipped catalog |
| Hashed state | — | None added; schema unchanged (35, or whatever `main` has at merge) |
| Worlds that move | — | None: no measured world contains a non-Terran structure, and no shipped consumer reads the changed attributes in any world (§3.2, §7) |
| Size | — | S: three packages, one of them S |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; **verify** not confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), layers `core`, `liberty`, `libertymulti`, `swarm`, `swarmmulti`, `void`, `voidmulti`, `balancemulti`, files `UnitData.xml`, `EffectData.xml`, `AbilData.xml` (the copy of the catalog-time audit, `~/scratch/combat_wp3_data/`). Chain `core → liberty → swarm → void → voidmulti` (audit §2.2). Citation `liberty UnitData:N` is line N of that layer's file | Every value below [DATA] |
| S2 | `CUnit` `Attributes` of each structure: the `<Attributes index="…" value="1"/>` lines inside its `liberty` element (table §2.3) | The attribute set of every structure the catalog models [DATA] |
| S3 | The same element in every later layer, read with a script that prints each layer's `CUnit id="X"` element and any `Attributes` or `parent=` line in it (`~/scratch/structure-attributes/attrs.py`, run with `python3 -I`) | **No later layer sets an attribute on any of the 49 structures.** None of them has a `parent=` [DATA] |
| S4 | `grep -n 'Attributes index=[^>]*value="0"'` over all layers' `UnitData` | Zeroed attributes exist only on `CreepTumor`, `CreepTumorBurrowed`, `CreepTumorQueen` (`voidmulti UnitData:277–278`, `:287–288`, `:293–294`; `balancemulti UnitData:225–226`, `:233–234`, `:238`: `Light 1`, `Armored 0`), the Disruptor, the Oracle, the Mothership Core and two critters. None is a catalog structure [DATA] |
| S5 | `core UnitData:23`, `<CUnit default="1">` | Sets no attribute; a structure's set is exactly its own lines [DATA] |
| S6 | `grep -ho 'AttributeBonus index="…"' */EffectData.xml`, then each `Biological`, `Mechanical`, `Psionic`, `Structure` bonus traced to its effect | The game's consumers of the changed attributes (§3.2): `PsionicShockwaveDamage` +10 Biological (Archon, `liberty EffectData:1358`, `swarm :1215`); `TwinGatlingCannons` +8 Mechanical (landed Viking, `voidmulti EffectData:1004`, `balancemulti :879`); `SnipeDamage` / `ChannelSnipeDamage` +25 / +40 Psionic (`libertymulti :23`, `voidmulti :1229`); `NukeDamage` +200 Structure (`liberty :1908`); `SporeCrawlerU` +Biological (air only); `MineDroneExplodeDamage` +20 Biological (`swarm :2897`) [DATA] |
| S7 | Ability target filters: `Repair` `Mechanical,Visible;Self,Enemy,…` (`liberty AbilData:1139`); `Transfusion` `Biological,Visible;…` (`liberty AbilData:3531`, `swarm :700`); `Snipe` / `ChannelSnipe` `Biological,Visible;Self,Structure,…` (`liberty AbilData:1260`, `voidmulti :1254`, `balancemulti :1093`) | Repair takes Terran structures and no Protoss or Zerg one; Transfusion takes Zerg structures; Snipe excludes every structure [DATA] |

### 2.2 The rules that follow

1. **Every structure has `Armored` and `Structure`** (S2, all 49 rows).
2. **Terran structures add `Mechanical`, Zerg structures add `Biological`, Protoss structures add nothing**
   (S2). Shields have nothing to do with it: no Protoss structure is `Psionic` and no Zerg structure is
   `Mechanical`.
3. **No structure has `Light`, `Psionic`, `Massive` or `Heroic`** among the 49 (S2, S3). The attribute is not
   fixed by kind, though: the 5.0.14 Creep Tumor is `Light`, `Biological`, `Structure` (S4, `liberty
   UnitData:8525–8527` with `voidmulti :277–278`). So the catalog must be free to author any attribute on a
   structure; the rule "a structure names `Structure`" is the only one the data never breaks.
4. **No mobile unit has `Structure`** (the catalog's 42 unit rows, `SC2_UNIT_ATTRIBUTES` of
   `catalog_reference_values_tests.rs`, sourced in plan `2026-11-05-sc2-deviation-fixes` B-47; no shipped bits
   value has bit 64).
5. **Mode variants keep the set**: Supply Depot Lowered (`liberty UnitData:5738–5740`), Spine and Spore Crawler
   Uprooted (`:8704–8706`, `:8841–8843`) and the Nydus Worm (`NydusCanal`, `:9169–9171`) have the same
   attributes as their structures. The catalog models none of these modes (no `modes` on a structure row), so
   nothing follows from it today (§14).

### 2.3 Values (S2; `liberty UnitData` element line, then its `Attributes` lines)

Bits: Armored 2, Biological 4, Mechanical 8, Psionic 16, Structure 64.

| Analogue (catalog row) | Element | Attributes lines | Set | Bits |
|---|---|---|---|---|
| Command Center (`foundry_core_dome`) | `:10359` | `:10400–10402` | Armored, Mechanical, Structure | 74 |
| Orbital Command (`orbital_relay_dome`) | `:10502` | `:10546–10548` | Armored, Mechanical, Structure | 74 |
| Planetary Fortress (`planetary_bulwark_dome`) | `:10644` | `:10684–10686` | Armored, Mechanical, Structure | 74 |
| Supply Depot (`capacitor_dome`) | `:5645` | `:5681–5683` | Armored, Mechanical, Structure | 74 |
| Refinery (`plasma_siphon_dome`) | `:5758` | `:5794–5796` | Armored, Mechanical, Structure | 74 |
| Barracks (`assembly_bay_dome`) | `:5821` | `:5856–5858` | Armored, Mechanical, Structure | 74 |
| Engineering Bay (`mechanics_lab_dome`) | `:5954` | `:5988–5990` | Armored, Mechanical, Structure | 74 |
| Bunker (`fortress_bunker_dome`) | `:6140` | `:6174–6176` | Armored, Mechanical, Structure | 74 |
| Sensor Tower (`radar_beacon_dome`) | `:9436` | `:9470–9472` | Armored, Mechanical, Structure | 74 |
| Missile Turret (`flak_array_dome`) | `:6021` | `:6052–6054` | Armored, Mechanical, Structure | 74 |
| Factory (`machining_plant_dome`) | `:6220` | `:6256–6258` | Armored, Mechanical, Structure | 74 |
| Ghost Academy (`covert_ops_nexus_dome`) | `:9755` | `:9791–9793` | Armored, Mechanical, Structure | 74 |
| Starport (`aerospace_hangar_dome`) | `:6353` | `:6389–6391` | Armored, Mechanical, Structure | 74 |
| Armory (`weapons_arsenal_dome`) | `:6490` | `:6524–6526` | Armored, Mechanical, Structure | 74 |
| Fusion Core (`fusion_reactor_dome`) | `:10901` | `:10933–10935` | Armored, Mechanical, Structure | 74 |
| Tech Lab (`tech_module_pod`) | `:6624` (`TechLab`; `BarracksTechLab` `:6689` has `parent="TechLab"`) | `:6662–6664` | Armored, Mechanical, Structure | 74 |
| Reactor (`duplicator_module_pod`) | `:6561` (`Reactor`; `BarracksReactor` `:437`, its own lines `:463–465`, the same set) | `:6599–6601` | Armored, Mechanical, Structure | 74 |
| Nexus (`citadel_nexus_dome`) | `:4166` | `:4208–4209` | Armored, Structure | 66 |
| Pylon (`psi_pylon_dome`) | `:4318` | `:4353–4354` | Armored, Structure | 66 |
| Assimilator (`rift_siphon_dome`) | `:4103` | `:4140–4141` | Armored, Structure | 66 |
| Gateway (`warp_gate_dome`) | `:4376` | `:4412–4413` (Warp Gate `:4488–4489`, the same) | Armored, Structure | 66 |
| Forge (`crystalline_forge_dome`) | `:4519` | `:4554–4555` | Armored, Structure | 66 |
| Photon Cannon (`tesla_sentinel_dome`) | `:11038` | `:11072–11073` | Armored, Structure | 66 |
| Shield Battery (`harmonic_recharge_dome`) | `:1563` | `:1585–1586` | Armored, Structure | 66 |
| Cybernetics Core (`cybernetics_conduit_dome`) | `:10800` | `:10835–10836` | Armored, Structure | 66 |
| Twilight Council (`templar_conclave_dome`) | `:4587` | `:4622–4623` | Armored, Structure | 66 |
| Stargate (`stellar_portal_dome`) | `:9623` | `:9658–9659` | Armored, Structure | 66 |
| Robotics Facility (`robotic_foundry_dome`) | `:9552` | `:9584–9585` | Armored, Structure | 66 |
| Templar Archives (`psionic_archives_dome`) | `:4650` (`TemplarArchive`) | `:4685–4686` | Armored, Structure | 66 |
| Dark Shrine (`void_sanctuary_dome`) | `:9494` | `:9528–9529` | Armored, Structure | 66 |
| Robotics Bay (`robotic_assembly_bay_dome`) | `:9822` | `:9855–9856` | Armored, Structure | 66 |
| Fleet Beacon (`celestial_beacon_dome`) | `:9691` | `:9726–9727` | Armored, Structure | 66 |
| Hatchery (`ancient_tree_dome`) | `:8193` | `:8232–8234` | Armored, Biological, Structure | 70 |
| Lair (`canopy_grove_dome`) | `:8274` | `:8314–8316` | Armored, Biological, Structure | 70 |
| Hive (`primal_world_tree_dome`) | `:8354` | `:8394–8396` | Armored, Biological, Structure | 70 |
| Extractor (`spore_vent_dome`) | `:8133` | `:8167–8169` | Armored, Biological, Structure | 70 |
| Spawning Pool (`spawning_hollow_dome`) | `:8873` | `:8904–8906` | Armored, Biological, Structure | 70 |
| Evolution Chamber (`metamorphic_chamber_dome`) | `:8430` | `:8462–8464` | Armored, Biological, Structure | 70 |
| Spine Crawler (`thorn_lasher_dome`) | `:8601` | `:8640–8642` | Armored, Biological, Structure | 70 |
| Spore Crawler (`spore_spire_dome`) | `:8735` | `:8772–8774` | Armored, Biological, Structure | 70 |
| Roach Warren (`carapace_mound_dome`) | `:11751` | `:11783–11785` | Armored, Biological, Structure | 70 |
| Baneling Nest (`caustic_nursery_dome`) | `:9952` | `:9984–9986` | Armored, Biological, Structure | 70 |
| Hydralisk Den (`spine_den_dome`) | `:8934` | `:8967–8969` | Armored, Biological, Structure | 70 |
| Lurker Den (`burrow_spire_dome`) | `:631` (`LurkerDenMP`) | `:650–652` | Armored, Biological, Structure | 70 |
| Spire (`chitin_spire_dome`) | `:8996` | `:9029–9031` | Armored, Biological, Structure | 70 |
| Greater Spire (`apex_spire_dome`) | `:9067` | `:9100–9102` | Armored, Biological, Structure | 70 |
| Infestation Pit (`infestation_nursery_dome`) | `:2032` | `:2046–2048` | Armored, Biological, Structure | 70 |
| Nydus Network (`subterranean_network_dome`) | `:9885` | `:9918–9920` | Armored, Biological, Structure | 70 |
| Ultralisk Cavern (`behemoth_den_dome`) | `:9201` | `:9233–9235` | Armored, Biological, Structure | 70 |

Slugs are given to help the reader find rows; the tests find rows by analogue, never by slug (C5).

### 2.4 Special cases checked

| Case | Verdict | Why |
|---|---|---|
| Planetary Fortress, Orbital Command | As the Command Center | S2; no later layer changes them (S3) |
| Supply Depot lowered | Same set; no lowered mode in the catalog | §2.2 rule 5 |
| Bunker, Missile Turret, Photon Cannon, Spine and Spore Crawler | As their faction's other structures; armament plays no part | S2 |
| Spine and Spore Crawler uprooted | Same set; no uprooted mode in the catalog | §2.2 rule 5 |
| Shield Battery, Nexus (shields, energy) | Armored, Structure: **not Psionic** | S2; today's derivation adds Psionic |
| Extractor, Assimilator, Refinery | Each as its faction (Biological, nothing, Mechanical) | S2 |
| Tech Lab, Reactor | The generic `TechLab` / `Reactor` elements, which the per-producer variants inherit or repeat | S2; `BarracksTechLab` `parent="TechLab"` |
| Creep Tumor | **Light**, Biological, Structure (Armored cleared in `voidmulti`) | S4; not in the catalog (no creep, B-63), recorded for that work (§14 F-3) |
| Gateway / Warp Gate | The same set; one catalog row | S2 |
| The `Robotic`, `Hover`, `Summoned` attributes | Not on any structure; not modelled by the engine | S2; the `Attribute` enum has the game's eight damage-relevant attributes only |

---

## 3. Diagnosis

### 3.1 The derivation

`crates/sim_core/src/catalog.rs`, `impl StructureArchetypeDef`, `pub fn into_archetype` (the structure arm of
the catalog conversion):

```rust
let mut attributes = AttributeFlags::STRUCTURE | AttributeFlags::ARMORED;
if self.max_shields > 0.0 {
    attributes |= AttributeFlags::PSIONIC;
} else {
    attributes |= AttributeFlags::MECHANICAL;
}
```

1. **It reads the wrong field.** Shields are not an attribute. The rule is right only for the Terran rows (no
   shields, Mechanical), wrong for every Protoss row (Psionic added) and every Zerg row (Mechanical instead of
   Biological): 32 of 49 rows.
2. **It is identity in code** (charter §3.3). A faction's attribute set is decided by a branch on a vital, not
   read from the catalog; changing a structure's attributes needs a code change and a rebuild, which the
   charter forbids. `StructureArchetypeDef` has no `attributes` field; the struct is also not
   `deny_unknown_fields`, so an `attributes:` line added to the data alone would be **silently ignored** — the
   data and the code must change in one commit (§11).
3. **Units are authored, but not validated.** `UnitArchetypeDef::attributes` is an `AttributeFlagsDef` read
   with `from_bits_truncate` (`catalog.rs`, `AttributeFlagsDef::into_flags`): an unknown bit vanishes without an
   error, and an empty set loads. An empty set is a real defect: every shipped weapon's `target_flags` is
   `(bits: 255)` and the engine admits a target only if `target_flags.intersects(target_arch.attributes)`
   (`target_acquisition.rs`, the four `intersects` calls; `combat_resolution.rs` twice), so an archetype with no
   attribute could never be attacked. Nothing stops a unit row from naming `Structure` either, which would make
   combat treat it as a structure (structure strike) while the effect filters do not (§3.2).

### 3.2 Who reads attributes today (the consumers)

| Reader | What it reads | Affected by this plan? |
|---|---|---|
| `combat.rs` `raw_strike_damage` → `AttributeBonuses::against` | Each weapon's bonus against each attribute the target has | No change today: shipped bonuses are against `Armored`, `Light` and `Massive` only on ground weapons (`units.ron`: 18 weapons; `structures.ron`: the Spine Crawler's +5 Armored); the one `Biological` bonus is the Spore Crawler's air weapon (`structures.ron`, `Spore Volley`), which cannot strike a ground structure. Every structure stays `Armored` and gains no `Light` or `Massive` |
| `combat.rs` `structure_strike_against` (B-82) | `STRUCTURE` | No change: every structure keeps it (now required by validation) |
| `target_acquisition.rs`, `combat_resolution.rs`, `combat.rs` `can_engage`-style checks | `target_flags.intersects(attributes)` | No change: all 52 shipped `target_flags` are 255 and no set becomes empty |
| `effects/filter.rs` `passes_filter` | `without_attributes` (the only shipped use: `concussive_shells_slow`, `[Massive]`) and `is_structure` by **shape** (`ShapeKind::BuildingDome`) | No change: no structure gains `Massive`; the shape rule is untouched (D-3 keeps it equal to the attribute) |
| `sim_fixtures/src/sandbox.rs` `Roles::resolve` | `LIGHT`, `ARMORED` on mobile archetypes only (structures filtered out by shape) | No change |
| `client/src/lab_ui.rs` (Visual Lab panel) | Labels for `LIGHT`, `ARMORED`, `BIOLOGICAL`, `MECHANICAL`, `STRUCTURE`; the unarmed line "Structure: …" on `STRUCTURE` | **Visible change**: a Zerg structure shows `[ BIOLOGICAL ]` instead of `[ MECHANICAL ]`. Protoss structures look the same (the panel never showed `PSIONIC`) |
| `catalog_reference_values_tests.rs` `is_structure` | `STRUCTURE` | No change; now guaranteed by validation |
| Checksum (`checksum/encoding.rs`) | Not hashed: archetypes are catalog data; only each weapon's `AttributeBonuses` are encoded | None |

**Consumers in the game that the engine does not model yet** (S6, S7), each of which today's derivation would
get wrong the day it arrives: the Archon's +10 against Biological (would miss every Zerg structure), the landed
Viking's +8 against Mechanical (would hit every Zerg structure), Repair (would repair Zerg structures),
Transfusion (would refuse Zerg structures), and Snipe's +25 Psionic (excluded from structures by its filter, so
harmless). The Nuke's +200 against `Structure` is unaffected (every structure keeps `Structure`).

So B-88 is **latent**: a wrong value in the catalog that no rule reads yet. That is why no world moves (§7), and
why the plan's evidence is the data table and the validation, not a measurement.

---

## 4. Design decisions

### D-1. An authored, required `attributes` list on every structure row

- **Field.** `attributes: Vec<sim_types::Attribute>` on `StructureArchetypeDef` (`catalog.rs`), **without**
  `#[serde(default)]`, after `sc2_inspiration`'s neighbours where the vitals begin, with the `///` doc "The
  structure's attributes (`CUnit` `Attributes`), by name; must name `Structure`." Written in the data as
  ```ron
  // 5.0.14 Attributes: Armored, Biological, Structure (liberty UnitData:8232-8234).
  attributes: [Armored, Biological, Structure],
  ```
  on each of the 49 rows, with the row's §2.3 lines in the comment.
- **Why names, not bits.** Units author `(bits: N)`, which a reader must decode; weapons' `attribute_bonuses`
  and effect filters' `without_attributes` already name `Attribute` variants. A list of names is the readable
  form, parses into the same enum, and RON refuses an unknown name at parse time (e.g. `Robotic`). Converting
  `units.ron` to names is out of scope (§14 F-1).
- **Why required.** There is no right default: every faction's set differs, and a default is how the bug came
  about. A row without the field fails to parse with the serde error naming the field and row.
- **Conversion.** `attributes: crate::catalog::spells::attribute_flags(&self.attributes)` (the existing
  `pub(crate)` union helper in `catalog/spells.rs`) replaces the six lines of §3.1. The structure's
  `into_archetype` shrinks by five lines (it is 106 lines with braces today; the C8 count excludes blanks and
  comments, **verify** it stays under 100 after the edit, else extract the vital block into a helper).
- **Zero recompilation** (charter §3.3): editing a row's list changes the archetype in the same binary (P-3).

### D-2. Validation of a structure's attributes

A private `fn structure_attributes(definition: &str, attributes: &[Attribute]) -> SimResult<()>` in
`catalog/validation.rs`, called from `StructureArchetypeDef::validate` (83 lines today; one line added). In this
order, each a `SimCoreError::CatalogInvalidValue { field: "attributes", .. }`:

1. Empty: `"must name at least one attribute"`.
2. A name twice: field `attributes.<Name>`, `"listed twice"` (the form of `validate_attribute_bonuses`).
3. No `Structure`: `"a structure has the Structure attribute"`.

**The allowed set** is the `Attribute` enum, enforced by the parser. No further restriction (for example "no
`Light` on a structure") is made, because the game itself breaks it (the Creep Tumor, §2.2 rule 3); the only rule
the data never breaks is rule 3 above. Rule 1 is implied by rule 3 but kept for a clear message.

### D-3. `Structure` names exactly the structure rows (units validated from the other side)

A private `fn unit_attributes(definition: &str, bits: u16) -> SimResult<()>`, called from
`UnitArchetypeDef::validate`, with `AttributeFlagsDef` gaining `fn bits(&self) -> u16` (the raw value of each
variant, before truncation):

1. `AttributeFlags::from_bits(bits)` is `None`: `"names a bit no attribute has"` (today silently dropped).
2. Empty: `"must name at least one attribute; an archetype without one matches no weapon's target flags"`.
3. Contains `STRUCTURE`: `"only a structure row has the Structure attribute"`.
4. Shape `ShapeKind::BuildingDome`: field `shape`, `"only a structure row has a building shape"`. A unit row
   authors its `shape: ShapeKind` directly (`UnitArchetypeDef::shape`; every shipped row is `UnitSphere`), and
   nothing refuses a building shape on it today, so without this rule the effect filters would treat such a unit
   as a structure while combat did not.

With D-2, **for every catalog that loads: an archetype has `STRUCTURE` if and only if it came from a structure
row, which is exactly when its shape is `ShapeKind::BuildingDome`** (the structure conversion is the only code
that builds that shape from a structure row, and rule 4 keeps unit rows off it). So `combat.rs`'s attribute test and
`effects/filter.rs`'s shape test are two readings of one fact, and neither is changed. The `///` docs of
`AttributeFlags::STRUCTURE` (`sim_types/src/enums.rs`) and `UnitArchetype::attributes` state the invariant.

All 42 shipped unit rows pass (bits 4 to 42, none with 64); the test catalogs that author units use 1, 13, 17
and the `attribute_bonus_tests` values 1, 2, 32, 34 (`grep -rn "attributes: (bits"` over `crates/`; **verify**
in WP1 that none authors 0).

### D-4. The `Structure` attribute: it exists, and what consumes it

`Attribute::Structure` / `AttributeFlags::STRUCTURE` (bit 64) exist already (`sim_types/src/weapon.rs`,
`enums.rs`). Nothing new is added to the data model. Its consumers, all tested after this plan (§12):

1. **Structure strike** (B-82, `combat.rs` `structure_strike_against`): the Baneling analogue's 80-damage strike
   applies to a target with `STRUCTURE`.
2. **Attribute bonus against `Structure`** (`AttributeBonuses`, index 6): no shipped weapon uses it (the game's
   only one is the Nuke, S6), but the damage formula applies it like any other.
3. **The Visual Lab panel** (`lab_ui.rs`): its label and its unarmed line.
4. **Effect filters do not read it**; they read the shape, equal to it by D-3.

### D-5. Hashed state: none; schema unchanged

Archetypes are catalog data and are not encoded (`checksum/encoding.rs` encodes each weapon's
`AttributeBonuses`, not a unit's `AttributeFlags`). `CHECKSUM_SCHEMA_VERSION` stays 35; no package claims a
schema number. If another branch merges first, this plan follows `main`'s schema.

### D-6. Presentation

- **The battle HUD shows no attributes** (`battle_hud/` reads none; `grep -rn attributes crates/client/src` finds
  only `lab_ui.rs`). No change.
- **The Visual Lab panel** shows the corrected set without a code change (§3.2). Its missing labels for
  `PSIONIC`, `MASSIVE` and `HEROIC` are a separate gap (§14 F-2); not added here, so the `client` crate is not
  touched.

### D-7. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-88's last column: "plan [`2026-11-23-structure-attributes`](docs/plans/2026-11-23-structure-attributes.md)" | WP0 |
| `DESIGN.md` §4 | B-88 **fixed**, with the commit and the evidence link; "latent: no shipped rule read the wrong attributes" | WP2 |
| `docs/03-sc2-mechanics-reference.md` §1.2 | One sentence under the attribute list: structures are Armored and Structure, plus Mechanical (Terran) or Biological (Zerg); authored per row in `structures.ron`; `Structure` exactly on structure rows (validated) | WP2 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | B-88's row: "fixed by plan `2026-11-23-structure-attributes`" | WP2 |
| New `docs/evidence/<date>-structure-attributes.md` | §7's comparisons; the before/after bits per faction | WP1 creates; WP2 completes |
| `docs/plans/README.md` | This plan's row: `Proposed` (with the plan), `Active` (WP1), `Done` (WP2) | plan, WP1, WP2 |

---

## 5. Determinism and checksum

- **No arithmetic changes.** The field is a list of enum names converted to a `u16` bit set at load; no float
  (C7's allowlist does not move, **verify** with `cargo gate quick`).
- **No hashed state, no schema change** (D-5).
- **Goldens are expected unchanged.** `golden_replay.ron` places two `foundry_core_dome` (Terran: 74 before and
  after) and builds nothing; `golden_replay_orders.ron` places no structure; `sim_fixtures`'
  `DEFAULT_SANDBOX_CHECKSUM` uses the shipped catalog's Terran-analogue roles (all loadouts' town hall is
  `foundry_core_dome`, `start_loadouts.ron`). And even a Protoss or Zerg structure would take the same damage
  (§3.2).
- **If a golden moves anyway**, the package stops and the move is proved, not just re-recorded:
  1. Trace the first moved checkpoint to an entity (the checkpoint diff or `matchup_trace`'s state dump) and name
     the rule that read an attribute.
  2. With the derivation restored by a temporary local edit (the six lines of §3.1 in place of the authored
     list), the tree reproduces every recorded checksum of both goldens on native debug and release and on
     `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`,
     the path the tree really has, not a symlink).
  3. Then the change goes back in, the checksums are re-recorded under a header "Re-recorded for plan
     2026-11-23-structure-attributes WP1: <cause>. No other behaviour changed: with the derivation restored,
     every checkpoint stays as recorded before", and the four builds reproduce the new values. Schema: unchanged
     (no encoding moved); if an encoding had to move, the next free number at merge.

  No such move is expected; an unexplained one is a defect.
- **wasm.** No dependency changes; the simulation crates still build for `wasm32-unknown-unknown`.
- **Cost.** None per tick; one list per structure row at load.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Each property holds for every row of the shipped catalog, and the tests
find rows by analogue or by property, never by slug.

- **P-1 (the game's sets).** Every shipped structure's attributes equal its 5.0.14 analogue's `CUnit`
  `Attributes` (§2.3, 49 rows, each with its source); every structure row has a table row and every table row a
  structure.
- **P-2 (one meaning of "structure").** For every archetype of the shipped catalog: `STRUCTURE` ⇔ it was loaded
  from `structures.ron` ⇔ `ShapeKind::BuildingDome`.
- **P-3 (data, not code).** Shields do not influence attributes: a catalog copy that gives a shielded structure
  `[Armored, Mechanical, Structure]` loads it with exactly those, and one that gives an unshielded structure
  `[Armored, Light, Structure]` makes a shipped weapon with a `Light` bonus (found by property) deal that bonus
  to it, in the same binary.
- **P-4 (validation).** A structure row with the field missing fails to parse; with an empty list, a duplicate
  or no `Structure` it is refused with `CatalogInvalidValue` naming `attributes`. A unit row with an unknown
  bit, no bit, `Structure` or a `BuildingDome` shape is refused likewise.
- **P-5 (consumers follow the data).** For every shipped structure and a scratch weapon with a bonus `b` against
  attribute `A` (each of the eight in turn), the raw strike damage is `base + b` exactly when the structure
  authors `A`; a bonus against `Structure` reaches every structure and no unit; the structure strike reaches
  every structure and no unit.
- **P-6 (no world moves).** Every world of §7 is byte-identical before and after each package; goldens and the
  sandbox checksum unchanged.

---

## 7. Worlds that move, and why no prototype was built

**None.** Two independent reasons, either sufficient:

1. **Only Terran structures exist in measured worlds,** and their set is unchanged (74). The matrix
   (`matchup_matrix.ron`) and battles 31 and 33 spawn units only (no `_dome` in the matrix); the showcase plays
   `battle_automaton` on both sides (`assets/scenarios/showcase.ron`), whose town hall is `foundry_core_dome`;
   the client skirmish plays `standard_automaton` on both sides (`assets/scenarios/skirmish.ron`), and its pilot
   raises a Terran structure.
2. **No shipped rule reads the changed attributes** (§3.2), so even a constructed world with a Protoss or Zerg
   structure under fire takes the same damage.

A prototype would measure a constant; each package proves P-6 by running the same commands on its tree against
WP0's.

**Commands** (the regeneration plan's; binaries built once per tree, run from that tree so its assets are read;
outputs under `~/scratch/structure-attributes/<wp>/`; the scripts copied from `~/scratch/resource-footprints/scripts/measure.sh`
and `~/scratch/regeneration/scripts/compare.sh` into `~/scratch/structure-attributes/scripts/`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/structure-attributes/bin-<wp>/
~/scratch/structure-attributes/scripts/measure.sh ~/scratch/structure-attributes/bin-<wp> <tree> ~/scratch/structure-attributes/world-<wp>
~/scratch/structure-attributes/scripts/compare.sh ~/scratch/structure-attributes/world-wp0 ~/scratch/structure-attributes/world-<wp>
# expected: "differing=0"
```

(`measure.sh`: the matrix with seeds of record and 16, battle 31 on 100 seeds, battle 33 on 64, the showcase on
its default seed and seeds 1 to 8 for 6,000 ticks.) Run in the background, one at a time across worktrees
(charter §7).

| World | WP1 | Why |
|---|---|---|
| Matrix (seeds of record and 16), battle 31 (100 seeds), battle 33 (64 seeds) | byte-identical | No structure |
| Showcase, default seed and seeds 1–8 | byte-identical | Terran town halls only; set unchanged |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | §5 |
| Client skirmish match (`skirmish_match_tests`) | stage log identical | Terran on both sides |
| Constructed worlds (§12) | P-3, P-5 | The only place the change shows |

A difference in any row is a defect of the package, not a re-record (§8).

---

## 8. Stop rules

1. **WP1 changes catalog values only.** P-1 to P-5 hold; §7's worlds byte-identical; goldens unchanged. Every
   failing existing test is one that authors a structure row (§10's test files) and gains its `attributes`
   line; any other failure stops the package.
2. **No derivation remains.** After WP1, `grep -n "max_shields > 0.0" crates/sim_core/src/catalog.rs` finds no
   attribute code, and no production code sets `AttributeFlags::{PSIONIC, MECHANICAL, BIOLOGICAL}` on an
   archetype (the reads in `lab_ui.rs` are presentation).
3. **No identity in code** (C5): the tests find structures by analogue (`sc2_inspiration`) or property.
4. **A golden move** follows §5's proof or stops the package.

---

## 9. Client consequences

- **No code change.** The battle HUD and tooltips show no attributes (D-6).
- **Visual Lab**: a Zerg structure's panel reads `[ ARMORED ] [ BIOLOGICAL ] [ STRUCTURE ]` (was
  `[ MECHANICAL ]`); Protoss and Terran panels look the same. No client test asserts these labels
  (`grep -rn "MECHANICAL\|BIOLOGICAL" crates/client/tests` is empty on `d2671045`).
- The client's tests load the shipped catalog (`asset_baker` and `client` `runtime_catalog_tests` copy the
  shipped files), so they parse the new field; the package gate runs them as usual.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP2 | B-88 pointer; then fixed |
| `docs/plans/README.md` | plan, WP1, WP2 | Row `Proposed` → `Active` → `Done` |
| `crates/sim_core/src/catalog.rs` | WP1 | `StructureArchetypeDef::attributes`; the conversion (D-1); `AttributeFlagsDef::bits`; `///` doc of `UnitArchetype::attributes` (D-3's invariant) |
| `crates/sim_core/src/catalog/validation.rs` | WP1 | `structure_attributes`, `unit_attributes`, their calls, unit tests; the inline structure rows of its existing tests gain the field |
| `crates/sim_types/src/enums.rs` | WP1 | `///` docs of `AttributeFlags::STRUCTURE` (D-3) and, for accuracy, of the bit list (no code) |
| `assets/catalog/structures.ron` | WP1 | `attributes:` on the 49 rows, each with its §2.3 citation |
| `crates/sim_core/tests/catalog_reference_values_tests.rs` | WP1 | `SC2_STRUCTURE_ATTRIBUTES` (49 rows) and its test; the P-2 property test |
| `crates/sim_core/tests/attribute_bonus_tests.rs` | WP1 | P-5 |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP1 | P-3; its inline structure rows gain the field |
| `crates/sim_core/tests/structure_radius_tests.rs`, `footprint_flag_tests.rs`, `faction_catalog_tests.rs` | WP1 | Their inline structure rows gain `attributes: [Armored, Structure]` (or the set of the analogue they imitate); no assertion changes |
| `docs/03-sc2-mechanics-reference.md`, `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP2 | D-7 |
| `docs/evidence/<date>-structure-attributes.md` (new) | WP1, WP2 | §7 |

Nothing under `crates/sim_core/src/movement/` changes; a contact-solver branch works there. `crates/client` is not
touched. Any other file that authors a structure row (`grep -rln structure_type_id crates assets` on the
package's tree; on `d2671045` the five test files above, `validation.rs` and `structures.ron`) joins WP1 with the
same one-line change.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first); the implementation agent writes the failing tests first and pastes
their red run in the hand-back, runs `cargo gate quick` and the tests its change needs, and hands back; a verifier
runs `cargo gate package` (Q0061). No `unwrap`; `expect` only with `invariant:`; no catalog identity in
production Rust (C5); tests find archetypes by analogue or property. Check `docs/questions/open/` before the last
commit. The three packages may run as one branch and one agent session, one commit each, with one `package`
gate on the final tree (charter §4.3: test a tree once).

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-88 points to this plan (D-7).
- Measure WP0's tree (§7) for the baseline.
- **Gate:** docs-only.

### WP1: attributes as data (S, `sim_types` docs, `sim_core`, catalog)

Failing tests first (red on WP0's tree):
- `catalog_reference_values_tests::b88_structure_attributes_follow_starcraft` (P-1): `check_table("structure
  attributes", true, &SC2_STRUCTURE_ATTRIBUTES, &[], |a| [f32::from(a.attributes.bits())])`, the 49 rows of
  §2.3 with "Armored+Biological+Structure, liberty UnitData:8232" style sources. Red today on 32 rows.
- `catalog_reference_values_tests::the_structure_attribute_marks_exactly_the_structures` (P-2): for every
  archetype, `attributes.contains(STRUCTURE) == matches!(shape, BuildingDome { .. })`, and the count of such
  archetypes equals the number of rows of the shipped `structures.ron` (`STRUCTURES_RON` parsed by the public
  catalog loader). Green before and after (a guard).
- `validation.rs` unit tests (P-4): `a_structure_without_the_attributes_field_does_not_parse`,
  `a_structure_with_no_attribute_is_refused`, `a_structure_attribute_listed_twice_is_refused`,
  `a_structure_without_the_structure_attribute_is_refused`, `a_unit_with_the_structure_attribute_is_refused`,
  `a_unit_with_no_attribute_is_refused`, `a_unit_with_an_unknown_attribute_bit_is_refused` (bits 256),
  `a_unit_with_a_building_shape_is_refused`, each
  asserting the `CatalogInvalidValue` field and reason (the parse test asserts the parse error names
  `attributes`).
- `attribute_bonus_tests` (P-5): `a_bonus_reaches_exactly_the_structures_that_author_its_attribute` (each of the
  eight attributes, every shipped structure: the public `calculate_hit_damage_upgraded` (`raw_strike_damage` is
  `pub(crate)`) with no upgrades, for a scratch `WeaponSpec` of base damage 50 with and without a bonus `b` = 7
  against that attribute; the two results differ by exactly `b` when the structure authors the attribute and are
  equal otherwise, armour and shields being the same in both; red today: a `Biological` bonus misses the Zerg
  structures and a `Psionic` bonus hits the Protoss ones); `a_structure_bonus_reaches_every_structure_and_no_unit`;
  `the_structure_strike_reaches_every_structure_and_no_unit` (the shipped weapon with a `structure_strike`, found
  by property).
- `runtime_catalog_tests` (P-3): `an_edited_structure_attribute_list_is_read_as_written` (the first shielded
  structure row given `[Armored, Mechanical, Structure]` loads with exactly those bits: no Psionic from shields;
  the first unshielded row given `[Armored, Light, Structure]` takes a shipped `Light` bonus).

Then the change: D-1 to D-3 and the 49 rows, in **one commit** with the tests (the field is required, and the
struct ignores unknown fields, so data without code is silently ignored and code without data does not load).
Existing test rows gain the field (§10). **Acceptance:** §8 rules 1 to 3; §7 worlds byte-identical; evidence
created. Plan row `Active`.

Commit `fix(sim_core): structure attributes are authored in structures.ron and validated (structure-attributes
WP1, B-88)`; the message states that no world of §7 and no golden moved, and why (§3.2).

### WP2: bookkeeping (XS)

- D-7's records: B-88 fixed with the commit; doc 03; the deviation-fixes pointer; the evidence completed; this
  plan's `## As built`; README row `Done`.
- Merge `main`; `cargo gate package` once on the final tree (by the verifier).

---

## 12. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Data | P-1: 49 rows equal the 5.0.14 sets, sourced | WP1: `catalog_reference_values_tests` |
| Invariant | P-2: `STRUCTURE` ⇔ structure row ⇔ building shape | WP1: `catalog_reference_values_tests` |
| Validation | P-4: required, non-empty, no duplicate, `Structure` on structures only; units' unknown and empty bits and building shapes | WP1: `catalog/validation.rs` unit tests |
| Zero recompilation | P-3: the list is read as written; shields play no part | WP1: `runtime_catalog_tests` |
| Consumers | P-5: attribute bonuses (all eight), the `Structure` bonus, the structure strike | WP1: `attribute_bonus_tests` |
| Worlds | P-6: §7's table | WP1 (measure script); goldens and sandbox in the package gate |
| Charter subjects | `sim_core`'s subjects of `docs/guides/test-gates.md` (shield and armour spillover, a whole match, golden replay) keep passing unchanged | package gate |

No new test file is added, so `crates/gate/suites.ron` needs no entry.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | A test or tool elsewhere authors a structure row without the field | Its catalog fails to parse | §10's grep on the package's tree; the parse error names the field and the row |
| R-2 | A test or tool authors a unit with bits 0, an unknown bit, or 64 | Refused by D-3 | The grep of D-3; such a row was already a defect (unattackable or a false structure); fix the row, never loosen the rule |
| R-3 | A test catalog authors a unit row with a `BuildingDome` shape | Refused by D-3 rule 4 | `grep -rn "BuildingDome" crates/*/tests` on `d2671045`: no test catalog authors one on a unit row. Tests that set a building shape on a loaded archetype (`production_transactions_tests.rs`, `terrain_radius_tests.rs`, client `battle_cast_tests.rs`, `battle_mode_tests.rs`) mutate after validation and are unaffected; the invariant is a property of every catalog that loads, not of hand-edited archetypes |
| R-4 | An unmodelled consumer assumed the old sets | None found (§3.2) | The consumer table is exhaustive over `grep -rn "attributes\|AttributeFlags::" crates/*/src` on `d2671045` |
| R-5 | A golden or the sandbox moves | Checksum churn | §5's proof; not expected |
| R-6 | The contact-solver branch merges first | Conflict | It works under `movement/`; this plan's files are the catalog and tests; it claims no schema |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: `units.ron` attributes as names.** Units keep `(bits: N)`; converting their 42 rows to
  `[Light, Biological]` lists would make one format for both files. It changes no value and is a pure data and
  parser change; left for a data-format pass so this plan's diff stays one rule. D-3 already closes the silent
  truncation that made bits risky.
- **F-2: the Visual Lab's missing labels** (`PSIONIC`, `MASSIVE`, `HEROIC`). A presentation gap, not a
  deviation; touches `client` only.
- **F-3: Creep Tumor attributes** (Light, Biological, Structure; S4). For the creep work (B-63), whose tumor row
  will author them; D-2 already allows `Light` on a structure.
- **F-4: mode variants** (Supply Depot lowered, uprooted crawlers, Nydus Worm): same sets (§2.2 rule 5); a mode
  that changes attributes would need a mode-level override, not needed by any 5.0.14 structure.
- **F-5: `deny_unknown_fields` on `StructureArchetypeDef`.** Its absence is why a data-only fix would have been
  silently ignored (§3.1). Adding it is a hardening of the whole structure schema (the struct carries
  `#[allow(dead_code)]` fields whose authoring must be checked first) and belongs to a catalog-schema pass.
- **The unmodelled consumers** (Archon, landed Viking, Repair, Transfusion, Nuke; S6, S7): each belongs to its
  unit's or ability's plan, which will now find correct attributes.

---

## 15. Owner questions

None. Every value is sourced [DATA] (§2.3), and the one design choice with a visible effect (the Visual Lab's
Zerg label) follows the data.

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read as data with `grep`, `sed` and a short read-only Python script (S3), run with `python3 -I`
  from a directory outside the data.
- No web source was needed: the data answers every row.
- No prototype (§7).

---

## As built

- **WP0** `84c2767d`: B-88 points to this plan; the WP0 tree measured into `~/scratch/structure-attributes/world-wp0`.
- **WP1** `5e3ac1ea`: as D-1 to D-3, in one commit with the data and the tests. `attributes:` sits on each row
  just before `max_hp` (where the vitals begin), under a comment citing its `liberty UnitData` lines; all 49 rows
  were checked against those lines by script before the edit. `compare.sh world-wp0 world-wp1`: `differing=0`
  over 420 files; both goldens and `DEFAULT_SANDBOX_CHECKSUM` unchanged; schema unchanged.
- **WP2**: D-7's records; [evidence](../evidence/2026-11-23-structure-attributes.md).

Differences from the text above, none of which changes a decision:

1. **C8.** One added line took `UnitArchetypeDef::validate` to 101 counted lines. Its Overclock checks moved,
   unchanged, into a new `validate_overclock`. The unit's attribute and shape checks are one method,
   `validate_attributes`, which calls D-3's `unit_attributes(definition, bits)` and then applies rule 4 (shape).
2. **P-5's expectation is read from the data text.** `a_bonus_reaches_exactly_the_structures_that_author_its_attribute`
   reads each row's `attributes:` list from `sim_fixtures::STRUCTURES_RON` itself, not from the loaded archetype, so
   the test does not check the conversion against its own result. The red run was taken with the 49 data lines in
   place and the WP0 code: the code ignored them silently, as §3.1 predicted, and P-1, P-3 and P-5 failed.
3. **Damage function.** P-5 calls `calculate_hit_damage_upgraded` with default `PlayerUpgradeState`s and no
   shields; P-3 uses `calculate_hit_damage`, which does the same.
4. **One more P-4 test**, `a_structure_may_author_any_attribute_besides_structure`: all eight, and `Structure`
   alone, both load (D-2 restricts nothing else).
5. **No other file authored a structure row** without the field: the test files of §10 build their rows from the
   shipped text, so they needed no change. No test catalog authors unit attributes 0, an unknown bit, 64 or a
   building shape.
6. **`sim_types` docs.** Besides `STRUCTURE`, the docs of `MECHANICAL` and `PSIONIC` were reworded (`MECHANICAL`
   now names structures too); no code changed.
