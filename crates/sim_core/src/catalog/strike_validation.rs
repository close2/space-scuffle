//! Catalog validation of strike magazines, impact spawns (plan `2026-11-28-broodlings` D-3,
//! D-4) and death spawns (plan `2026-12-01-death-spawns-and-hangar-commands` D-1).
//!
//! # Responsibility
//! Refuses, with a typed [`SimCoreError::CatalogInvalidValue`] naming the slug and field, every
//! authored strike magazine, impact spawn and death spawn the mechanisms cannot run: a magazine's
//! counts and ticks out of range or on a unit whose weapons it cannot gate, an impact spawn on a
//! weapon that launches no missile or whose buffer could grow unbounded, a death spawn creating
//! none or more than [`MAX_DEATH_SPAWN_COUNT`] units, and a spawned unit that could enter the
//! world otherwise than by spawning, could not act, or would spawn in turn.
//!
//! # How it fits
//! The per-row rules run in `UnitArchetypeDef::validate` and `WeaponSpecDef::validate` before a
//! row converts; the rules that relate a weapon to the unit it spawns run once the whole bundle
//! is loaded, from [`UnitCatalog::validate_production_references`], since a spawned unit's
//! training structures are in the structures file.
//!
//! # Invariants
//! In a loaded complete catalog: a strike magazine has a capacity of at least 1, starts with at
//! most its capacity, builds in at least one tick and arms in fewer ticks than it builds (so at
//! most one charge arms at a time); its unit has no hangar, and every weapon of its unit releases
//! at most `capacity` strikes at once, launches no magazine and neither delays nor spaces its
//! strikes. An impact spawn is on a missile weapon, creates at least one unit per strike within
//! a nonnegative search range, at most [`MAX_SPAWNS_PER_MISSILE`] per missile, and its unit is a
//! mobile, armed, supply-free unit with no hangar, strike magazine, impact spawn or death spawn of
//! its own, trained by no structure. A death spawn creates 1 to [`MAX_DEATH_SPAWN_COUNT`] units
//! within a nonnegative search range, and its unit is a mobile, supply-free unit (no structure)
//! with no hangar, strike magazine, impact spawn or death spawn of its own, trained by no
//! structure; any archetype may carry one.

use super::validation::{invalid, nonnegative};
use super::{DeathSpawnSpecDef, UnitArchetype, UnitArchetypeDef, UnitCatalog, WeaponSpecDef};
use crate::error::{SimCoreError, SimResult};
use sim_types::{ShapeKind, SimScalar, WeaponSpec};

/// Most units one missile may create (`per_strike × attacks_count`), which bounds the combat
/// phase's retained spawn buffer per finishing missile (plan `2026-11-28-broodlings` D-4).
pub(crate) const MAX_SPAWNS_PER_MISSILE: u16 = 8;

/// Most units one death creates (plan `2026-12-01-death-spawns-and-hangar-commands` D-1): above
/// 5.0.14's largest `SpawnCount` of a death response (9, the town halls' Broodlings), and the
/// bound the client's render pool reserves per dying unit (D-8).
pub(crate) const MAX_DEATH_SPAWN_COUNT: u8 = 16;

impl DeathSpawnSpecDef {
    /// D-1, per row: 1 to [`MAX_DEATH_SPAWN_COUNT`] units, a nonnegative search range. The rules
    /// that need the created unit run once the bundle is loaded
    /// ([`UnitCatalog::validate_production_references`]).
    pub(super) fn validate(&self, definition: &str) -> SimResult<()> {
        nonnegative(
            definition,
            "capabilities.death_spawn.search_range",
            f64::from(self.search_range),
        )?;
        if !(1..=MAX_DEATH_SPAWN_COUNT).contains(&self.count) {
            return Err(invalid(
                definition,
                "capabilities.death_spawn.count",
                "a death spawn creates 1 to 16 units",
            ));
        }
        Ok(())
    }
}

impl WeaponSpecDef {
    /// D-4: an impact spawn creates at least one unit per strike, searches a nonnegative range,
    /// is on a missile weapon (an instant weapon has no finish) that launches no magazine, and
    /// creates at most [`MAX_SPAWNS_PER_MISSILE`] units per missile.
    pub(super) fn validate_impact_spawn(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let Some(spawn) = &self.impact_spawn else {
            return Ok(());
        };
        let field = |name: &str| format!("{prefix}.impact_spawn.{name}");
        nonnegative(
            definition,
            &field("search_range"),
            f64::from(spawn.search_range),
        )?;
        let rules: [(&str, bool, &'static str); 4] = [
            (
                "per_strike",
                spawn.per_strike >= 1,
                "an impact spawn creates at least one unit per strike",
            ),
            (
                "unit",
                self.projectile_speed.is_some(),
                "an instant weapon has no missile to finish",
            ),
            (
                "unit",
                self.magazine_launch.is_none(),
                "a launch weapon launches no missile to finish",
            ),
            (
                "per_strike",
                u16::from(spawn.per_strike) * u16::from(self.attacks_count)
                    <= MAX_SPAWNS_PER_MISSILE,
                "one missile creates at most 8 units",
            ),
        ];
        match rules.into_iter().find(|(_, holds, _)| !holds) {
            Some((name, _, reason)) => Err(invalid(definition, &field(name), reason)),
            None => Ok(()),
        }
    }
}

impl UnitArchetypeDef {
    /// D-3: a strike magazine's counts and ticks in range (capacity at least 1, start at most the
    /// capacity, a build of at least one tick, an arming shorter than the build), no hangar
    /// beside it, and every weapon of the unit, modes included, releasing at most `capacity`
    /// strikes at once with no magazine launch and no delayed or spaced strikes (a release spends
    /// its charges once, on the release tick).
    pub(super) fn validate_strike_magazine(&self) -> SimResult<()> {
        let Some(magazine) = self.capabilities.strike_magazine else {
            return Ok(());
        };
        let rules: [(&str, bool, &'static str); 5] = [
            (
                "capacity",
                magazine.capacity >= 1,
                "a strike magazine holds at least one charge",
            ),
            (
                "start",
                magazine.start <= magazine.capacity,
                "a strike magazine starts with at most its capacity",
            ),
            (
                "build_ticks",
                magazine.build_ticks >= 1,
                "a charge takes at least one tick to build",
            ),
            (
                "arming_ticks",
                magazine.arming_ticks < magazine.build_ticks,
                "a charge arms in fewer ticks than the next one builds",
            ),
            (
                "hangar",
                self.capabilities.hangar.is_none(),
                "a unit has a hangar or a strike magazine, not both",
            ),
        ];
        if let Some((name, _, reason)) = rules.into_iter().find(|(_, holds, _)| !holds) {
            let field = if name == "hangar" {
                "capabilities.hangar".to_owned()
            } else {
                format!("capabilities.strike_magazine.{name}")
            };
            return Err(invalid(&self.slug, &field, reason));
        }
        let mode_weapons = self
            .modes
            .iter()
            .chain(&self.capabilities.modes)
            .flat_map(|mode| [&mode.ground_weapon_override, &mode.air_weapon_override])
            .filter_map(|weapon| weapon.as_ref().and_then(Option::as_ref));
        let weapons = [&self.weapon, &self.ground_weapon, &self.air_weapon]
            .into_iter()
            .flatten()
            .chain(mode_weapons);
        for weapon in weapons {
            let rules: [(&str, bool, &'static str); 3] = [
                (
                    "attacks_count",
                    weapon.attacks_count <= magazine.capacity,
                    "a release spends at most the magazine's capacity",
                ),
                (
                    "magazine_launch",
                    weapon.magazine_launch.is_none(),
                    "a strike magazine gates strikes, not launches",
                ),
                (
                    "strike_interval_ticks",
                    weapon.first_strike_delay_ticks == 0 && weapon.strike_interval_ticks == 0,
                    "a strike magazine's release lands its strikes together",
                ),
            ];
            if let Some((name, _, reason)) = rules.into_iter().find(|(_, holds, _)| !holds) {
                return Err(invalid(
                    &self.slug,
                    &format!("{}.{name}", weapon.name),
                    reason,
                ));
            }
        }
        Ok(())
    }
}

impl UnitCatalog {
    /// D-4: every impact spawn's unit, in the complete catalog, is a mobile unit (no structure)
    /// with a striking weapon, no supply cost, no hangar, strike magazine or impact spawn of its
    /// own (no chain), and trained by no structure: it enters the world only by spawning.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] on the spawning unit's `<weapon>.impact_spawn.unit`
    /// with the broken rule.
    pub(super) fn validate_impact_spawns(&self) -> SimResult<()> {
        for archetype in self.archetypes() {
            for weapon in weapons_of(archetype) {
                let Some(spawn) = weapon.impact_spawn else {
                    continue;
                };
                let fail = |reason| -> SimCoreError {
                    invalid(
                        &archetype.slug,
                        &format!("{}.impact_spawn.unit", weapon.name),
                        reason,
                    )
                };
                let unit = self
                    .get(spawn.unit)
                    .map_err(|_| fail("the spawned unit names no unit of the catalog"))?;
                if let Some(reason) = self.spawned_unit_fault(unit, true) {
                    return Err(fail(reason));
                }
            }
        }
        Ok(())
    }

    /// Plan `2026-12-01-death-spawns-and-hangar-commands` D-1: every death spawn's unit, in the
    /// complete catalog, is a mobile unit (no structure) with no supply cost, no hangar, strike
    /// magazine, impact spawn or death spawn of its own (no chain), and trained by no structure.
    /// The host may be any archetype.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] on the host's `capabilities.death_spawn.unit` with
    /// the broken rule.
    pub(super) fn validate_death_spawns(&self) -> SimResult<()> {
        for archetype in self.archetypes() {
            let Some(spawn) = archetype.capabilities.death_spawn else {
                continue;
            };
            let fail = |reason| -> SimCoreError {
                invalid(&archetype.slug, "capabilities.death_spawn.unit", reason)
            };
            let unit = self
                .get(spawn.unit)
                .map_err(|_| fail("the spawned unit names no unit of the catalog"))?;
            if let Some(reason) = self.spawned_unit_fault(unit, false) {
                return Err(fail(reason));
            }
        }
        Ok(())
    }

    /// The first rule the spawned archetype `unit` breaks, if any: D-4 of the broodlings plan for
    /// an impact spawn (`armed`: it must have a striking weapon), D-1 of the death-spawns plan
    /// for a death spawn (no weapon required).
    fn spawned_unit_fault(&self, unit: &UnitArchetype, armed: bool) -> Option<&'static str> {
        let striking = weapons_of(unit).any(|weapon| weapon.magazine_launch.is_none());
        let spawns = weapons_of(unit).any(|weapon| weapon.impact_spawn.is_some());
        let trained = self
            .archetypes()
            .any(|trainer| trainer.trains_units.contains(&unit.slug));
        let rules: [(bool, &'static str); 9] = [
            (
                !matches!(unit.shape, ShapeKind::BuildingDome { .. }),
                "the spawned unit is a unit, not a structure",
            ),
            (unit.max_speed > SimScalar::ZERO, "the spawned unit moves"),
            (
                striking || !armed,
                "the spawned unit has a weapon of its own",
            ),
            (unit.supply_cost == 0, "the spawned unit takes no supply"),
            (
                unit.capabilities.hangar.is_none(),
                "the spawned unit has no hangar",
            ),
            (
                unit.capabilities.strike_magazine.is_none(),
                "the spawned unit has no strike magazine",
            ),
            (!spawns, "the spawned unit spawns nothing in turn"),
            (
                unit.capabilities.death_spawn.is_none(),
                "the spawned unit leaves nothing when it dies",
            ),
            (
                !trained,
                "the spawned unit is trained by no structure, only created by its spawn",
            ),
        ];
        rules
            .into_iter()
            .find(|(holds, _)| !holds)
            .map(|(_, reason)| reason)
    }
}

/// Every weapon of `archetype`: its weapon, ground and air weapons and its modes' weapons.
fn weapons_of(archetype: &UnitArchetype) -> impl Iterator<Item = &WeaponSpec> {
    let modes = archetype
        .modes
        .iter()
        .flat_map(|mode| [&mode.ground_weapon_override, &mode.air_weapon_override])
        .filter_map(|weapon| weapon.as_ref().and_then(Option::as_ref));
    [
        &archetype.weapon,
        &archetype.ground_weapon,
        &archetype.air_weapon,
    ]
    .into_iter()
    .flatten()
    .chain(modes)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    //! B-10 (plan `2026-11-28-broodlings` §6.1): every rule of D-3 and D-4 refused with the field
    //! it names; the shipped catalog validates.
    use super::super::{CatalogFile, DeathSpawnSpecDef, ImpactSpawnDef, UnitArchetypeDef};
    use crate::error::{SimCoreError, SimResult};
    use sim_types::{MagazineLaunch, MoverType, SpawnOrder, StrikeMagazineSpec, TimedLifeSpec};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn field<T>(result: SimResult<T>) -> String {
        match result {
            Err(SimCoreError::CatalogInvalidValue { field, .. }) => field,
            Err(other) => format!("other error: {other}"),
            Ok(_) => "accepted".to_owned(),
        }
    }

    /// The first shipped air unit whose only weapon is a missile weapon of two strikes or fewer
    /// that lands them together, with no mode, made a well-formed spawner: a strike magazine
    /// `(2, 2, 45, 9)` and an impact spawn of `unit`.
    fn spawner_row(unit: &str) -> UnitArchetypeDef {
        let file: CatalogFile = ron::from_str(include_str!("../../../../assets/catalog/units.ron"))
            .expect("invariant: the shipped units.ron parses");
        let mut row = file
            .units
            .into_iter()
            .find(|row| {
                row.mover_type == MoverType::Air
                    && row.modes.is_empty()
                    && row.capabilities.modes.is_empty()
                    && row.capabilities.hangar.is_none()
                    && row.air_weapon.is_none()
                    && row.weapon.as_ref().is_some_and(|w| {
                        w.projectile_speed.is_some()
                            && w.attacks_count <= 2
                            && w.strike_interval_ticks == 0
                            && w.first_strike_delay_ticks == 0
                    })
            })
            .expect("a shipped air unit with a missile weapon");
        row.ground_weapon = None;
        row.capabilities.strike_magazine = Some(StrikeMagazineSpec {
            capacity: 2,
            start: 2,
            build_ticks: 45,
            arming_ticks: 9,
        });
        let weapon = row.weapon.as_mut().expect("invariant: found armed above");
        weapon.impact_spawn = Some(ImpactSpawnDef {
            unit: unit.to_owned(),
            per_strike: 1,
            search_range: 5.0,
            timed_life: Some(TimedLifeSpec {
                ticks: 89,
                random_extra_ticks: 5,
            }),
            order: SpawnOrder::AttackTargetOrPoint,
        });
        row
    }

    /// The well-formed spawner row is accepted, and each broken row rule of D-3 and D-4 is
    /// refused naming its field.
    #[test]
    #[allow(clippy::too_many_lines, reason = "one table of cases, each a rule")]
    fn every_row_rule_is_refused_naming_its_field() {
        type Edit = fn(&mut UnitArchetypeDef);
        let row = spawner_row("x");
        let weapon = row.weapon.as_ref().expect("armed").name.clone();
        assert_eq!(field(row.validate()), "accepted");
        let cases: [(Edit, String); 12] = [
            (
                |r| r.capabilities.strike_magazine.as_mut().expect("m").capacity = 0,
                "capabilities.strike_magazine.capacity".to_owned(),
            ),
            (
                |r| r.capabilities.strike_magazine.as_mut().expect("m").start = 3,
                "capabilities.strike_magazine.start".to_owned(),
            ),
            (
                |r| {
                    let m = r.capabilities.strike_magazine.as_mut().expect("m");
                    m.build_ticks = 0;
                    m.arming_ticks = 0;
                },
                "capabilities.strike_magazine.build_ticks".to_owned(),
            ),
            (
                |r| {
                    r.capabilities
                        .strike_magazine
                        .as_mut()
                        .expect("m")
                        .arming_ticks = 45;
                },
                "capabilities.strike_magazine.arming_ticks".to_owned(),
            ),
            (
                |r| r.weapon.as_mut().expect("w").attacks_count = 3,
                format!("{weapon}.attacks_count"),
            ),
            (
                |r| r.weapon.as_mut().expect("w").strike_interval_ticks = 2,
                format!("{weapon}.strike_interval_ticks"),
            ),
            (
                |r| {
                    r.weapon.as_mut().expect("w").magazine_launch = Some(MagazineLaunch {
                        tries: 1,
                        interval_ticks: 1,
                    });
                },
                "weapon.base_damage".to_owned(),
            ),
            (
                |r| {
                    let s = r.weapon.as_mut().and_then(|w| w.impact_spawn.as_mut());
                    s.expect("s").per_strike = 0;
                },
                "weapon.impact_spawn.per_strike".to_owned(),
            ),
            (
                |r| {
                    let s = r.weapon.as_mut().and_then(|w| w.impact_spawn.as_mut());
                    s.expect("s").per_strike = 5;
                },
                "weapon.impact_spawn.per_strike".to_owned(),
            ),
            (
                |r| {
                    let s = r.weapon.as_mut().and_then(|w| w.impact_spawn.as_mut());
                    s.expect("s").search_range = -1.0;
                },
                "weapon.impact_spawn.search_range".to_owned(),
            ),
            (
                |r| {
                    let w = r.weapon.as_mut().expect("w");
                    w.projectile_speed = None;
                    w.missile_validation = sim_types::MissileValidation::default();
                    w.launch_visibility = sim_types::AttackVisibility::default();
                    w.missile_movers = Vec::new();
                    w.visual_kind = sim_types::WeaponVisualKind::LaserBeam;
                },
                "weapon.impact_spawn.unit".to_owned(),
            ),
            (
                |r| {
                    r.capabilities.hangar = Some(super::super::HangarSpecDef {
                        member: "x".to_owned(),
                        max_count: 8,
                        start_count: 4,
                        build_ticks: 214,
                        queue_size: 5,
                        auto_build: true,
                        auto_build_toggle: false,
                        leash: 12.0,
                        on_death: sim_types::HangarDeath::KillMembers,
                    });
                },
                "weapon".to_owned(),
            ),
        ];
        for (index, (edit, expected)) in cases.into_iter().enumerate() {
            let mut row = spawner_row("x");
            edit(&mut row);
            assert_eq!(field(row.validate()), expected, "case {index}");
        }
    }

    /// A strike magazine beside a hangar is refused by the magazine's own rule when the hangar's
    /// launch-weapon rule cannot see it first: a hangar unit with a launch weapon and a magazine.
    #[test]
    fn a_magazine_beside_a_hangar_is_refused() {
        let mut row = spawner_row("x");
        row.capabilities.hangar = Some(super::super::HangarSpecDef {
            member: "x".to_owned(),
            max_count: 8,
            start_count: 4,
            build_ticks: 214,
            queue_size: 5,
            auto_build: true,
            auto_build_toggle: false,
            leash: 12.0,
            on_death: sim_types::HangarDeath::KillMembers,
        });
        assert_eq!(field(row.validate_strike_magazine()), "capabilities.hangar");
    }

    /// D-4: an impact spawn's unit, in the complete shipped catalog, must be a mobile, armed,
    /// supply-free unit with no hangar, strike magazine or spawn of its own that no structure
    /// trains. Each broken rule is refused at the spawning weapon's `impact_spawn.unit`.
    #[test]
    fn every_spawned_unit_rule_is_refused_on_the_complete_catalog() -> TestResult {
        type Edit = fn(&mut crate::catalog::UnitArchetype);
        let mut catalogs = crate::test_support::shipped_catalogs();
        assert_eq!(field(catalogs.units.validate_impact_spawns()), "accepted");
        let units = &mut catalogs.units;
        let mut ground: Vec<_> = units
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Ground && a.weapon.is_some())
            .filter(|a| a.capabilities.modes.is_empty() && a.harvester.is_none())
            .filter(|a| a.capabilities.transport_capacity == 0)
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        let (spawner, spawned) = (ground[0], ground[1]);
        let spawned_slug = units.get(spawned)?.slug.clone();
        for trainer in units
            .archetypes()
            .map(|a| a.unit_type_id)
            .collect::<Vec<_>>()
        {
            units
                .get_mut(trainer)?
                .trains_units
                .retain(|slug| *slug != spawned_slug);
        }
        units.get_mut(spawned)?.supply_cost = 0;
        let weapon_name = {
            let arch = units.get_mut(spawner)?;
            let weapon = arch.weapon.as_mut().ok_or("armed")?;
            weapon.impact_spawn = Some(sim_types::ImpactSpawn {
                unit: spawned,
                per_strike: 1,
                search_range: sim_types::SimScalar::from_i32(5),
                timed_life: None,
                order: SpawnOrder::AttackTargetOrPoint,
            });
            weapon.name.clone()
        };
        let expected = format!("{weapon_name}.impact_spawn.unit");
        assert_eq!(field(units.validate_impact_spawns()), "accepted");
        let cases: [Edit; 5] = [
            |a| a.supply_cost = 2,
            |a| a.max_speed = sim_types::SimScalar::ZERO,
            |a| {
                a.weapon = None;
                a.ground_weapon = None;
                a.air_weapon = None;
            },
            |a| {
                a.capabilities.strike_magazine = Some(StrikeMagazineSpec {
                    capacity: 1,
                    start: 1,
                    build_ticks: 2,
                    arming_ticks: 1,
                });
            },
            |a| {
                let spawn = a.weapon.as_ref().and_then(|w| w.impact_spawn).or(Some(
                    sim_types::ImpactSpawn {
                        unit: a.unit_type_id,
                        per_strike: 1,
                        search_range: sim_types::SimScalar::ZERO,
                        timed_life: None,
                        order: SpawnOrder::AttackTargetOrPoint,
                    },
                ));
                if let Some(weapon) = a.weapon.as_mut() {
                    weapon.impact_spawn = spawn;
                }
            },
        ];
        for (index, edit) in cases.iter().enumerate() {
            let saved = units.get(spawned)?.clone();
            edit(units.get_mut(spawned)?);
            assert_eq!(
                field(units.validate_impact_spawns()),
                expected,
                "case {index}"
            );
            *units.get_mut(spawned)? = saved;
        }
        let trainer = units
            .archetypes()
            .find(|a| !a.trains_units.is_empty())
            .map(|a| a.unit_type_id)
            .ok_or("a trainer")?;
        units.get_mut(trainer)?.trains_units.push(spawned_slug);
        assert_eq!(field(units.validate_impact_spawns()), expected);
        Ok(())
    }

    /// An impact spawn whose slug names no unit of the file is refused when the row converts.
    #[test]
    fn an_unknown_spawned_slug_is_refused_at_load() {
        let units = include_str!("../../../../assets/catalog/units.ron");
        let factions = crate::test_support::shipped_factions();
        let row = spawner_row("no_such_unit");
        let mut file: CatalogFile = ron::from_str(units).expect("invariant: shipped units parse");
        let slug = row.slug.clone();
        let weapon = row.weapon.as_ref().expect("armed").name.clone();
        let index = file
            .units
            .iter()
            .position(|unit| unit.slug == slug)
            .expect("invariant: the row is shipped");
        file.units[index] = row;
        let ids = file
            .units
            .iter()
            .map(|def| (def.slug.clone(), sim_types::UnitTypeId(def.unit_type_id)))
            .collect();
        let classes = super::super::collision_classes::CollisionClassTable::from_units(&file.units)
            .expect("invariant: shipped classes");
        let converted = file
            .units
            .swap_remove(index)
            .into_archetype(&factions, &classes, &ids);
        assert_eq!(field(converted), format!("{weapon}.impact_spawn.unit"));
    }

    /// A well-formed authored death spawn creating `unit`: 5.0.14's `ZergBuildingDies9` (plan
    /// `2026-12-01-death-spawns-and-hangar-commands` §2.2).
    fn death_spawn_def(unit: &str) -> DeathSpawnSpecDef {
        DeathSpawnSpecDef {
            unit: unit.to_owned(),
            count: 9,
            delay_ticks: 9,
            search_range: 0.0,
            timed_life: Some(TimedLifeSpec {
                ticks: 143,
                random_extra_ticks: 5,
            }),
            disabled_while_constructing: true,
        }
    }

    /// The shipped structures file, parsed.
    fn structures_file() -> super::super::StructureCatalogFile {
        ron::from_str(include_str!("../../../../assets/catalog/structures.ron"))
            .expect("invariant: the shipped structures.ron parses")
    }

    /// DS-11 (plan `2026-12-01-death-spawns-and-hangar-commands` D-1), per row: a count of 1 to 16
    /// and a nonnegative range are accepted on a unit's capabilities and on a structure; a count
    /// of 0 or 17 and a negative range are refused naming the field; a structure row without the
    /// field reads `None`.
    #[test]
    fn every_death_spawn_row_rule_is_refused_naming_its_field() {
        type Edit = fn(&mut DeathSpawnSpecDef);
        let cases: [(Edit, &str); 5] = [
            (|_| {}, "accepted"),
            (|d| d.count = 16, "accepted"),
            (|d| d.count = 0, "capabilities.death_spawn.count"),
            (|d| d.count = 17, "capabilities.death_spawn.count"),
            (
                |d| d.search_range = -0.5,
                "capabilities.death_spawn.search_range",
            ),
        ];
        let mut file = structures_file();
        let structure = file.structures.swap_remove(0);
        assert!(
            structure.death_spawn.is_none(),
            "an omitted field reads None"
        );
        let mut structure = Some(structure);
        for (index, (edit, expected)) in cases.into_iter().enumerate() {
            let mut spawn = death_spawn_def("x");
            edit(&mut spawn);
            let mut unit = spawner_row("x");
            unit.capabilities.death_spawn = Some(spawn.clone());
            assert_eq!(field(unit.validate()), expected, "unit, case {index}");
            let mut row = structure.take().expect("invariant: put back below");
            row.death_spawn = Some(spawn);
            assert_eq!(field(row.validate()), expected, "structure, case {index}");
            structure = Some(row);
        }
    }

    /// DS-11, on the complete catalog: a death spawn's unit must be a mobile, supply-free unit (no
    /// structure) with no hangar, strike magazine, impact spawn or death spawn of its own that no
    /// structure trains; an unarmed unit is accepted. Each broken rule is refused at the host's
    /// `capabilities.death_spawn.unit`; an unknown slug is refused when the structure converts.
    #[test]
    #[allow(clippy::too_many_lines, reason = "one table of cases, each a rule")]
    fn every_death_spawned_unit_rule_is_refused_on_the_complete_catalog() -> TestResult {
        type Edit = fn(&mut crate::catalog::UnitArchetype);
        let mut catalogs = crate::test_support::shipped_catalogs();
        let units = &mut catalogs.units;
        let mut ground: Vec<_> = units
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Ground && a.weapon.is_some())
            .filter(|a| a.capabilities.modes.is_empty() && a.harvester.is_none())
            .filter(|a| a.capabilities.transport_capacity == 0)
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        let spawned = ground[0];
        let spawned_slug = units.get(spawned)?.slug.clone();
        for trainer in units
            .archetypes()
            .map(|a| a.unit_type_id)
            .collect::<Vec<_>>()
        {
            units
                .get_mut(trainer)?
                .trains_units
                .retain(|slug| *slug != spawned_slug);
        }
        units.get_mut(spawned)?.supply_cost = 0;
        let mut domes: Vec<_> = units
            .archetypes()
            .filter(|a| matches!(a.shape, sim_types::ShapeKind::BuildingDome { .. }))
            .map(|a| a.unit_type_id)
            .collect();
        domes.sort();
        let host = domes[0];
        let host_slug = units.get(host)?.slug.clone();
        units.get_mut(host)?.capabilities.death_spawn = Some(sim_types::DeathSpawnSpec {
            unit: spawned,
            count: 9,
            delay_ticks: 9,
            search_range: sim_types::SimScalar::ZERO,
            timed_life: None,
            disabled_while_constructing: true,
        });
        assert_eq!(field(units.validate_death_spawns()), "accepted");
        let cases: [(Edit, &str); 7] = [
            (
                |a| {
                    a.weapon = None;
                    a.ground_weapon = None;
                    a.air_weapon = None;
                },
                "accepted",
            ),
            (|a| a.supply_cost = 2, "rejected"),
            (|a| a.max_speed = sim_types::SimScalar::ZERO, "rejected"),
            (
                |a| {
                    a.capabilities.strike_magazine = Some(StrikeMagazineSpec {
                        capacity: 1,
                        start: 1,
                        build_ticks: 2,
                        arming_ticks: 1,
                    });
                },
                "rejected",
            ),
            (
                |a| {
                    let unit = a.unit_type_id;
                    if let Some(weapon) = a.weapon.as_mut() {
                        weapon.impact_spawn = Some(sim_types::ImpactSpawn {
                            unit,
                            per_strike: 1,
                            search_range: sim_types::SimScalar::ZERO,
                            timed_life: None,
                            order: SpawnOrder::AttackTargetOrPoint,
                        });
                    }
                },
                "rejected",
            ),
            (
                |a| {
                    a.capabilities.death_spawn = Some(sim_types::DeathSpawnSpec {
                        unit: a.unit_type_id,
                        count: 1,
                        delay_ticks: 1,
                        search_range: sim_types::SimScalar::ZERO,
                        timed_life: None,
                        disabled_while_constructing: false,
                    });
                },
                "rejected",
            ),
            (
                |a| {
                    a.capabilities.hangar = Some(sim_types::HangarSpec {
                        member: a.unit_type_id,
                        max_count: 8,
                        start_count: 4,
                        build_ticks: 214,
                        queue_size: 5,
                        auto_build: true,
                        auto_build_toggle: false,
                        leash: sim_types::SimScalar::from_i32(12),
                        on_death: sim_types::HangarDeath::KillMembers,
                    });
                },
                "rejected",
            ),
        ];
        for (index, (edit, expected)) in cases.iter().enumerate() {
            let saved = units.get(spawned)?.clone();
            edit(units.get_mut(spawned)?);
            let expected = if *expected == "accepted" {
                "accepted".to_owned()
            } else {
                "capabilities.death_spawn.unit".to_owned()
            };
            assert_eq!(
                field(units.validate_death_spawns()),
                expected,
                "case {index}"
            );
            *units.get_mut(spawned)? = saved;
        }
        let saved = units.get(spawned)?.clone();
        units.get_mut(spawned)?.shape = units.get(host)?.shape;
        assert_eq!(
            field(units.validate_death_spawns()),
            "capabilities.death_spawn.unit",
            "a structure is refused"
        );
        *units.get_mut(spawned)? = saved;
        let trainer = units
            .archetypes()
            .find(|a| !a.trains_units.is_empty())
            .map(|a| a.unit_type_id)
            .ok_or("a trainer")?;
        units.get_mut(trainer)?.trains_units.push(spawned_slug);
        let refused = units.validate_death_spawns();
        assert!(
            matches!(&refused, Err(SimCoreError::CatalogInvalidValue { definition, field, .. })
                if *definition == host_slug && field == "capabilities.death_spawn.unit"),
            "{refused:?}"
        );
        let factions = crate::test_support::shipped_factions();
        let mut file = structures_file();
        let mut row = file.structures.swap_remove(0);
        row.death_spawn = Some(death_spawn_def("no_such_unit"));
        let converted = row.into_archetype(&factions, &std::collections::BTreeMap::new());
        assert_eq!(field(converted), "capabilities.death_spawn.unit");
        Ok(())
    }
}
