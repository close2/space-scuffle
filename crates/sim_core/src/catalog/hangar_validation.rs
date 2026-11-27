//! Catalog validation of hangars and launch weapons (plan `2026-11-28-carrier-interceptors`
//! D-2, D-4, D-8).
//!
//! # Responsibility
//! Refuses, with a typed [`SimCoreError::CatalogInvalidValue`] naming the slug and field, every
//! authored hangar, launch weapon and member the hangar mechanism cannot run: a hangar's counts
//! and leash out of range, a launch weapon that would strike, a hangar unit without exactly one
//! launch weapon or a launch weapon without a hangar, and a member that could enter the world
//! otherwise than through its hangar or be selected or ordered by a player.
//!
//! # How it fits
//! The per-row rules run in `UnitArchetypeDef::validate` and `WeaponSpecDef::validate` before a
//! row converts; the rules that relate a hangar to its member run once the whole bundle is loaded,
//! from [`UnitCatalog::validate_production_references`], since a member's training structures are
//! in the structures file.
//!
//! # Invariants
//! A loaded complete catalog's hangar units each launch members through one launch weapon, and
//! every member is a mobile, armed, supply-free, unselectable and order-untargetable unit with no
//! hangar, transport, harvester or trainer of its own.

use super::validation::{invalid, positive};
use super::{UnitArchetypeDef, UnitCatalog, WeaponSpecDef};
use crate::error::{SimCoreError, SimResult};
use sim_types::{AttackVisibility, MoverType, ShapeKind, SimScalar, WeaponVisualKind};

impl WeaponSpecDef {
    /// D-4: a launch weapon deals nothing and launches no missile: tries and interval at least
    /// 1; base damage 0, no attribute bonus, one strike, no splash, no projectile speed and no
    /// movers; no visual; `Hidden` damage and launch visibility.
    pub(super) fn validate_magazine_launch(&self, definition: &str, prefix: &str) -> SimResult<()> {
        let Some(launch) = self.magazine_launch else {
            return Ok(());
        };
        let field = |name: &str| format!("{prefix}.{name}");
        let rules: [(&str, bool, &'static str); 11] = [
            (
                "magazine_launch.tries",
                launch.tries >= 1,
                "a launch tries at least once",
            ),
            (
                "magazine_launch.interval_ticks",
                launch.interval_ticks >= 1,
                "tries are at least one tick apart",
            ),
            (
                "base_damage",
                SimScalar::from_f32_lossy(self.base_damage) == SimScalar::ZERO,
                "a launch weapon deals no damage",
            ),
            (
                "attribute_bonuses",
                self.attribute_bonuses.is_empty(),
                "a launch weapon deals no damage",
            ),
            (
                "attacks_count",
                self.attacks_count == 1,
                "a launch weapon releases once",
            ),
            (
                "splash",
                self.splash.is_none(),
                "a launch weapon deals no damage",
            ),
            (
                "projectile_speed",
                self.projectile_speed.is_none(),
                "a launch weapon launches no missile",
            ),
            (
                "missile_movers",
                self.missile_movers.is_empty(),
                "a launch weapon launches no missile",
            ),
            (
                "visual_kind",
                self.visual_kind == WeaponVisualKind::None,
                "a launch weapon's members are its visual",
            ),
            (
                "attack_visibility",
                self.attack_visibility == AttackVisibility::Hidden,
                "a launch weapon strikes nothing to reveal its unit",
            ),
            (
                "launch_visibility",
                self.launch_visibility == AttackVisibility::Hidden,
                "a launch weapon launches no missile to show",
            ),
        ];
        for (name, holds, reason) in rules {
            if !holds {
                return Err(invalid(definition, &field(name), reason));
            }
        }
        Ok(())
    }
}

impl UnitArchetypeDef {
    /// D-2, D-4: a hangar's counts and leash in range, a hangar unit whose every weapon is a
    /// launch weapon (one, or one per plane as the catalog authors a weapon of both planes) and
    /// no mode weapon, and a launch weapon only on a hangar unit.
    pub(super) fn validate_hangar(&self) -> SimResult<()> {
        let weapons = [&self.weapon, &self.ground_weapon, &self.air_weapon]
            .into_iter()
            .flatten();
        let (armed, launches) = weapons.fold((0, 0), |(armed, launches), weapon| {
            (
                armed + 1,
                launches + usize::from(weapon.magazine_launch.is_some()),
            )
        });
        let mode_weapons = self
            .modes
            .iter()
            .chain(&self.capabilities.modes)
            .flat_map(|mode| [&mode.ground_weapon_override, &mode.air_weapon_override])
            .filter_map(|weapon| weapon.as_ref().and_then(Option::as_ref));
        let mut mode_weapons = mode_weapons.peekable();
        let Some(hangar) = &self.capabilities.hangar else {
            if launches > 0 || mode_weapons.any(|weapon| weapon.magazine_launch.is_some()) {
                return Err(invalid(
                    &self.slug,
                    "weapon.magazine_launch",
                    "a launch weapon needs a hangar to launch from",
                ));
            }
            return Ok(());
        };
        let rules: [(&str, bool, &'static str); 6] = [
            (
                "max_count",
                hangar.max_count >= 1,
                "a hangar holds at least one member",
            ),
            (
                "start_count",
                hangar.start_count <= hangar.max_count,
                "a hangar starts with at most max_count members",
            ),
            (
                "queue_size",
                hangar.queue_size >= 1,
                "a hangar queues at least one build",
            ),
            (
                "build_ticks",
                hangar.build_ticks >= 1,
                "a build takes at least one tick",
            ),
            (
                "weapon",
                launches >= 1 && launches == armed,
                "every weapon of a hangar unit is a launch weapon",
            ),
            (
                "modes",
                mode_weapons.peek().is_none(),
                "no mode gives a hangar unit another weapon",
            ),
        ];
        for (name, holds, reason) in rules {
            if !holds {
                let field = if matches!(name, "weapon" | "modes") {
                    name.to_owned()
                } else {
                    format!("capabilities.hangar.{name}")
                };
                return Err(invalid(&self.slug, &field, reason));
            }
        }
        positive(&self.slug, "capabilities.hangar.leash", hangar.leash)
    }
}

impl UnitCatalog {
    /// D-2, D-8: every hangar's member, in the complete catalog, is a mobile unit (no structure)
    /// with a striking weapon, no hangar, transport or harvester of its own, no supply cost,
    /// neither selectable nor order-targetable, and trained by no structure: it enters the world
    /// only through its hangar. A hangar unit itself cannot be loaded into a transport.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] on the hangar unit's `capabilities.hangar.member`,
    /// or its `mover_type`, with the broken rule.
    pub(super) fn validate_hangar_members(&self) -> SimResult<()> {
        for archetype in self.archetypes() {
            let Some(hangar) = archetype.capabilities.hangar else {
                continue;
            };
            let fail = |reason| -> SimCoreError {
                invalid(&archetype.slug, "capabilities.hangar.member", reason)
            };
            if archetype.mover_type != MoverType::Air {
                return Err(invalid(
                    &archetype.slug,
                    "mover_type",
                    "a hangar unit flies, so no transport can load it",
                ));
            }
            let member = self
                .get(hangar.member)
                .map_err(|_| fail("the member names no unit of the catalog"))?;
            let striking = [&member.weapon, &member.ground_weapon, &member.air_weapon]
                .into_iter()
                .flatten()
                .any(|weapon| weapon.magazine_launch.is_none());
            let trained = self
                .archetypes()
                .any(|trainer| trainer.trains_units.contains(&member.slug));
            let rules: [(bool, &'static str); 10] = [
                (
                    !matches!(member.shape, ShapeKind::BuildingDome { .. }),
                    "the member is a unit, not a structure",
                ),
                (member.max_speed > SimScalar::ZERO, "the member moves"),
                (
                    member.capabilities.hangar.is_none(),
                    "the member has no hangar of its own",
                ),
                (
                    member.capabilities.transport_capacity == 0,
                    "the member carries no passengers",
                ),
                (member.harvester.is_none(), "the member harvests nothing"),
                (member.supply_cost == 0, "the member takes no supply"),
                (striking, "the member has a weapon of its own"),
                (!member.selectable, "the member is not selectable"),
                (
                    !member.order_targetable,
                    "the member is no target of a player's order",
                ),
                (
                    !trained,
                    "the member is trained by no structure, only built by its hangar",
                ),
            ];
            if let Some((_, reason)) = rules.into_iter().find(|(holds, _)| !holds) {
                return Err(fail(reason));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    //! H-10 (plan `2026-11-28-carrier-interceptors` §6.2): every rule of D-2, D-4 and D-8
    //! refused with the field it names; omitted fields read their defaults; the shipped catalog
    //! validates.
    use super::super::{CatalogFile, HangarSpecDef, UnitArchetypeDef};
    use crate::error::{SimCoreError, SimResult};
    use sim_types::{AttackVisibility, HangarDeath, MagazineLaunch, MoverType, WeaponVisualKind};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn field<T>(result: SimResult<T>) -> String {
        match result {
            Err(SimCoreError::CatalogInvalidValue { field, .. }) => field,
            Err(other) => format!("other error: {other}"),
            Ok(_) => "accepted".to_owned(),
        }
    }

    /// The shipped unit rows.
    fn rows() -> Vec<UnitArchetypeDef> {
        let file: CatalogFile = ron::from_str(include_str!("../../../../assets/catalog/units.ron"))
            .expect("invariant: the shipped units.ron parses");
        file.units
    }

    /// The first shipped air unit with an instant weapon.
    fn first_instant_air_row() -> UnitArchetypeDef {
        rows()
            .into_iter()
            .find(|row| {
                row.mover_type == MoverType::Air
                    && row.modes.is_empty()
                    && row.capabilities.modes.is_empty()
                    && row
                        .weapon
                        .as_ref()
                        .is_some_and(|w| w.projectile_speed.is_none())
            })
            .expect("a shipped air unit with an instant weapon")
    }

    /// That row's own weapon, which strikes.
    fn striking_weapon() -> Option<super::super::WeaponSpecDef> {
        first_instant_air_row().weapon
    }

    /// The first shipped air unit with an instant weapon, made a well-formed hangar unit: its
    /// weapon a launch weapon, its other weapons removed, a hangar of member `member`.
    fn hangar_row(member: &str) -> UnitArchetypeDef {
        let mut row = first_instant_air_row();
        let mut weapon = row.weapon.take().expect("armed");
        weapon.magazine_launch = Some(MagazineLaunch {
            tries: 8,
            interval_ticks: 7,
        });
        weapon.base_damage = 0.0;
        weapon.attribute_bonuses = Vec::new();
        weapon.attacks_count = 1;
        weapon.splash = None;
        weapon.missile_movers = Vec::new();
        weapon.visual_kind = WeaponVisualKind::None;
        weapon.attack_visibility = AttackVisibility::Hidden;
        weapon.launch_visibility = AttackVisibility::Hidden;
        row.weapon = Some(weapon);
        row.ground_weapon = None;
        row.air_weapon = None;
        row.capabilities.hangar = Some(HangarSpecDef {
            member: member.to_owned(),
            max_count: 8,
            start_count: 4,
            build_ticks: 214,
            queue_size: 5,
            auto_build: true,
            auto_build_toggle: false,
            leash: 12.0,
            on_death: HangarDeath::KillMembers,
        });
        row
    }

    /// D-2, D-4: the well-formed hangar row is accepted, and each broken rule is refused naming
    /// its field.
    #[test]
    fn every_hangar_row_rule_is_refused_naming_its_field() {
        type Edit = fn(&mut UnitArchetypeDef);
        assert_eq!(field(hangar_row("x").validate()), "accepted");
        let cases: [(Edit, &str); 16] = [
            (
                |r| r.capabilities.hangar.as_mut().expect("h").max_count = 0,
                "capabilities.hangar.max_count",
            ),
            (
                |r| r.capabilities.hangar.as_mut().expect("h").start_count = 9,
                "capabilities.hangar.start_count",
            ),
            (
                |r| r.capabilities.hangar.as_mut().expect("h").queue_size = 0,
                "capabilities.hangar.queue_size",
            ),
            (
                |r| r.capabilities.hangar.as_mut().expect("h").build_ticks = 0,
                "capabilities.hangar.build_ticks",
            ),
            (
                |r| r.capabilities.hangar.as_mut().expect("h").leash = 0.0,
                "capabilities.hangar.leash",
            ),
            (|r| r.ground_weapon = striking_weapon(), "weapon"),
            (
                |r| {
                    let weapon = r.weapon.as_mut().expect("w");
                    weapon.magazine_launch = None;
                    weapon.launch_visibility = AttackVisibility::Snapshot;
                },
                "weapon",
            ),
            (
                |r| r.weapon.as_mut().expect("w").base_damage = 5.0,
                "weapon.base_damage",
            ),
            (
                |r| r.weapon.as_mut().expect("w").attacks_count = 2,
                "weapon.attacks_count",
            ),
            (
                |r| r.weapon.as_mut().expect("w").projectile_speed = Some(1.0),
                "weapon.projectile_speed",
            ),
            (
                |r| r.weapon.as_mut().expect("w").visual_kind = WeaponVisualKind::Projectile,
                "weapon.visual_kind",
            ),
            (
                |r| r.weapon.as_mut().expect("w").attack_visibility = AttackVisibility::Snapshot,
                "weapon.attack_visibility",
            ),
            (
                |r| r.weapon.as_mut().expect("w").launch_visibility = AttackVisibility::Snapshot,
                "weapon.launch_visibility",
            ),
            (
                |r| {
                    r.weapon.as_mut().expect("w").magazine_launch = Some(MagazineLaunch {
                        tries: 0,
                        interval_ticks: 7,
                    });
                },
                "weapon.magazine_launch.tries",
            ),
            (
                |r| {
                    r.weapon.as_mut().expect("w").magazine_launch = Some(MagazineLaunch {
                        tries: 8,
                        interval_ticks: 0,
                    });
                },
                "weapon.magazine_launch.interval_ticks",
            ),
            (|r| r.capabilities.hangar = None, "weapon.magazine_launch"),
        ];
        for (edit, expected) in cases {
            let mut row = hangar_row("x");
            edit(&mut row);
            assert_eq!(field(row.validate()), expected);
        }
    }

    /// D-2, D-8: a hangar's member, in the complete shipped catalog, must be a mobile, armed,
    /// supply-free, unselectable, order-untargetable unit no structure trains; and the hangar
    /// unit flies. Each broken rule is refused at the member field.
    #[test]
    fn every_member_rule_is_refused_on_the_complete_catalog() -> TestResult {
        type Edit = fn(&mut crate::catalog::UnitArchetype);
        let mut catalogs = crate::test_support::shipped_catalogs();
        assert_eq!(field(catalogs.units.validate_hangar_members()), "accepted");
        let mut air: Vec<_> = catalogs
            .units
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Air && a.weapon.is_some())
            .filter(|a| a.capabilities.modes.is_empty() && a.harvester.is_none())
            .filter(|a| a.capabilities.transport_capacity == 0)
            .map(|a| a.unit_type_id)
            .collect();
        air.sort();
        let (host, member) = (air[0], air[1]);
        let units = &mut catalogs.units;
        units.get_mut(host)?.capabilities.hangar = Some(sim_types::HangarSpec {
            member,
            max_count: 8,
            start_count: 4,
            build_ticks: 214,
            queue_size: 5,
            auto_build: true,
            auto_build_toggle: false,
            leash: sim_types::SimScalar::from_i32(12),
            on_death: HangarDeath::KillMembers,
        });
        let member_slug = units.get(member)?.slug.clone();
        for trainer in units
            .archetypes()
            .map(|a| a.unit_type_id)
            .collect::<Vec<_>>()
        {
            units
                .get_mut(trainer)?
                .trains_units
                .retain(|slug| *slug != member_slug);
        }
        let well_formed = |units: &mut crate::catalog::UnitCatalog| -> TestResult {
            let arch = units.get_mut(member)?;
            arch.supply_cost = 0;
            arch.selectable = false;
            arch.order_targetable = false;
            Ok(())
        };
        well_formed(units)?;
        assert_eq!(field(units.validate_hangar_members()), "accepted");
        let cases: [Edit; 6] = [
            |a| a.supply_cost = 2,
            |a| a.selectable = true,
            |a| a.order_targetable = true,
            |a| a.max_speed = sim_types::SimScalar::ZERO,
            |a| a.capabilities.transport_capacity = 4,
            |a| {
                a.weapon = None;
                a.ground_weapon = None;
                a.air_weapon = None;
            },
        ];
        for (index, edit) in cases.iter().enumerate() {
            let saved = units.get(member)?.clone();
            edit(units.get_mut(member)?);
            assert_eq!(
                field(units.validate_hangar_members()),
                "capabilities.hangar.member",
                "case {index}"
            );
            *units.get_mut(member)? = saved;
        }
        let trainer = units
            .archetypes()
            .find(|a| !a.trains_units.is_empty())
            .map(|a| a.unit_type_id)
            .ok_or("a trainer")?;
        units.get_mut(trainer)?.trains_units.push(member_slug);
        assert_eq!(
            field(units.validate_hangar_members()),
            "capabilities.hangar.member"
        );
        units.get_mut(trainer)?.trains_units.pop();
        units.get_mut(host)?.mover_type = MoverType::Ground;
        assert_eq!(field(units.validate_hangar_members()), "mover_type");
        Ok(())
    }

    /// D-2, D-4, D-8: rows that omit the new fields read their defaults: no hangar, no magazine
    /// launch, selectable and order-targetable.
    #[test]
    fn omitted_fields_read_their_defaults() {
        let catalogs = crate::test_support::shipped_catalogs();
        for arch in catalogs.units.archetypes() {
            if arch.capabilities.hangar.is_some() {
                continue;
            }
            assert!(arch.launch_weapon().is_none(), "{}", arch.slug);
            let member = catalogs.units.archetypes().any(|h| {
                h.capabilities
                    .hangar
                    .is_some_and(|s| s.member == arch.unit_type_id)
            });
            if !member {
                assert!(arch.selectable && arch.order_targetable, "{}", arch.slug);
            }
        }
    }
}
