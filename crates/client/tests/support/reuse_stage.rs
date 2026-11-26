//! Stages a dirty decoy that leaves a battle and a new unit that arrives after it, so the new
//! unit's bubble reuses the decoy's parked render entity, or, after the pool's parked entities
//! are released, is spawned fresh (plan `2026-11-29-render-entity-pool` R-3, R-8).
//!
//! # Responsibility
//! - [`Kinds`]: the archetypes a stage uses, found by catalog property and media present.
//! - [`Script`]: one decoy and one arrival, their owners, and when each happens;
//!   [`Script::stage_decoy`] puts the decoy on the field as dirty as a bubble gets (another owner
//!   than the arrival, at 30 % life, attacking a durable unarmed enemy, cloaked so the effect
//!   system holds a look on it and moves its shell), and [`Script::arrive`] spawns the arrival at
//!   60 % life. The caller selects them through `BattleSelection`.
//!
//! # How tests use it
//! Include it with `#[path = "support/reuse_stage.rs"]`. The world is written directly, as the
//! other stages do; no order under test is issued here.
//!
//! # Invariants
//! - Every unit is found by property, never by slug.
//! - The decoy and the arrival are of different owners and different archetypes.

#![allow(clippy::expect_used)]

use std::path::Path;

use sim_core::SimWorld;
use sim_types::{
    EntityId, MoverType, PlayerId, ShapeKind, SimScalar, SimVec2, UnitOrder, UnitTypeId,
};

/// The local player.
pub const LOCAL: PlayerId = PlayerId(0);

/// The local player's enemy.
pub const ENEMY: PlayerId = PlayerId(1);

/// The archetypes a stage uses.
#[derive(Debug, Clone, Copy)]
pub struct Kinds {
    /// Armed mobile ground units with media, two of them.
    pub ground: [UnitTypeId; 2],
    /// An armed air unit with media that strikes the ground.
    pub air: UnitTypeId,
    /// A ground unit with media, disarmed and made durable by [`Kinds::find`]: the decoy's
    /// target.
    pub dummy: UnitTypeId,
    /// A structure with media that needs no resource node.
    pub structure: UnitTypeId,
}

/// Whether the client ships the media of unit `slug`.
fn presentable(assets: &Path, slug: &str) -> bool {
    assets
        .join("textures")
        .join(format!("{slug}.flipbook.ron"))
        .is_file()
}

impl Kinds {
    /// Finds the kinds in `world`'s catalog, and disarms the dummy and makes it durable there
    /// (the world's catalog copy only).
    pub fn find(world: &mut SimWorld, assets: &Path) -> Self {
        let catalog = &world.catalog;
        let plain = |a: &&sim_core::catalog::UnitArchetype| {
            a.max_speed > SimScalar::ZERO
                && a.capabilities.modes.is_empty()
                && a.modes.is_empty()
                && a.harvester.is_none()
                && a.capabilities.hangar.is_none()
                && a.capabilities.strike_magazine.is_none()
                && presentable(assets, &a.slug)
        };
        let mut ground: Vec<UnitTypeId> = catalog
            .archetypes()
            .filter(plain)
            .filter(|a| a.mover_type == MoverType::Ground && a.ground_weapon.is_some())
            .filter(|a| {
                a.ground_weapon
                    .as_ref()
                    .is_some_and(|w| w.impact_spawn.is_none())
            })
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        let mut air: Vec<UnitTypeId> = catalog
            .archetypes()
            .filter(plain)
            .filter(|a| a.mover_type == MoverType::Air && a.ground_weapon.is_some())
            .map(|a| a.unit_type_id)
            .collect();
        air.sort();
        let mut structures: Vec<UnitTypeId> = catalog
            .archetypes()
            .filter(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
            .filter(|a| !a.capabilities.is_gas_extractor && presentable(assets, &a.slug))
            .filter(|a| a.ground_weapon.is_none() && a.weapon.is_none())
            .map(|a| a.unit_type_id)
            .collect();
        structures.sort();
        let kinds = Self {
            ground: [ground[0], ground[1]],
            air: *air.first().expect("an armed air unit with media"),
            dummy: *ground.get(2).expect("a third ground unit with media"),
            structure: *structures.first().expect("a structure with media"),
        };
        let dummy = world.catalog.get_mut(kinds.dummy).expect("the dummy kind");
        dummy.weapon = None;
        dummy.ground_weapon = None;
        dummy.air_weapon = None;
        dummy.max_hp = SimScalar::from_i32(1_000_000);
        kinds
    }
}

/// What the decoy is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoy {
    /// A cloaked ground soldier attacking the dummy.
    Ground,
    /// A cloaked air unit attacking the dummy.
    Air,
    /// A structure, destroyed.
    Dome,
}

/// What arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// A ground unit of the arrival's owner.
    Ground,
    /// An air unit of the arrival's owner.
    Air,
}

/// One decoy and one arrival.
#[derive(Debug, Clone, Copy)]
pub struct Script {
    /// What leaves.
    pub decoy: Decoy,
    /// What arrives.
    pub arrival: Arrival,
    /// Ticks between the decoy's last presented tick and the arrival's first; 0 is the same
    /// frame.
    pub hidden_ticks: u32,
    /// Where the decoy stands, map units.
    pub decoy_at: SimVec2,
    /// Where the arrival appears, map units.
    pub arrival_at: SimVec2,
    /// The decoy's owner; its dummy belongs to the arrival's owner.
    pub decoy_owner: PlayerId,
    /// The arrival's owner, never the decoy's.
    pub arrival_owner: PlayerId,
}

/// The staged decoy.
#[derive(Debug, Clone, Copy)]
pub struct Staged {
    /// The decoy.
    pub decoy: EntityId,
    /// Its target.
    pub dummy: EntityId,
}

impl Script {
    /// The decoy's archetype.
    fn decoy_kind(&self, kinds: &Kinds) -> UnitTypeId {
        match self.decoy {
            Decoy::Ground => kinds.ground[0],
            Decoy::Air => kinds.air,
            Decoy::Dome => kinds.structure,
        }
    }

    /// The arrival's archetype, never the decoy's.
    pub fn arrival_kind(&self, kinds: &Kinds) -> UnitTypeId {
        match self.arrival {
            Arrival::Ground => kinds.ground[1],
            Arrival::Air => kinds.air,
        }
    }

    /// Places the decoy and its target (see the module docs).
    pub fn stage_decoy(&self, world: &mut SimWorld, kinds: &Kinds) -> Staged {
        let kind = self.decoy_kind(kinds);
        let decoy = if self.decoy == Decoy::Dome {
            let (cx, cy) = world
                .map
                .world_to_cell(self.decoy_at)
                .expect("the decoy's place is on the map");
            (0..6)
                .flat_map(|dx| (0..6).map(move |dy| (cx + dx, cy + dy)))
                .find_map(|(x, y)| world.spawn_structure(kind, self.decoy_owner, x, y).ok())
                .expect("open ground for a structure near the decoy's place")
        } else {
            world
                .spawn_unit(kind, self.decoy_owner, self.decoy_at)
                .expect("open ground at the decoy's place")
        };
        let dummy = world
            .spawn_unit(
                kinds.dummy,
                self.arrival_owner,
                self.decoy_at + SimVec2::from_i32(2, 0),
            )
            .expect("open ground beside the decoy");
        let max_hp = world.catalog.get(kind).expect("the decoy kind").max_hp;
        let unit = world.entities.get_mut(&decoy).expect("the decoy");
        unit.hp = max_hp * SimScalar::from_num(0.3);
        if self.decoy != Decoy::Dome {
            unit.is_cloaked = true;
            unit.issue_order(UnitOrder::attack(dummy), false)
                .expect("an attack order");
        }
        world
            .entities
            .get_mut(&dummy)
            .expect("the dummy")
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("a hold order");
        Staged { decoy, dummy }
    }

    /// Kills the decoy: it leaves the snapshot on the next tick.
    pub fn kill_decoy(world: &mut SimWorld, staged: Staged) {
        if let Some(unit) = world.entities.get_mut(&staged.decoy) {
            unit.hp = SimScalar::ZERO;
        }
    }

    /// Spawns the arrival at 60 % life, holding its position.
    pub fn arrive(&self, world: &mut SimWorld, kinds: &Kinds) -> EntityId {
        let kind = self.arrival_kind(kinds);
        let id = world
            .spawn_unit(kind, self.arrival_owner, self.arrival_at)
            .expect("open ground at the arrival's place");
        let max_hp = world.catalog.get(kind).expect("the arrival kind").max_hp;
        let unit = world.entities.get_mut(&id).expect("the arrival");
        unit.hp = max_hp * SimScalar::from_num(0.6);
        unit.issue_order(UnitOrder::HoldPosition, false)
            .expect("a hold order");
        id
    }
}
