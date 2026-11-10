//! Shared fixture of the effect-tree engine tests: a flat world with the shipped units and a
//! scratch spell catalog, plus helpers to place units and start walks.
//!
//! Units are picked from the shipped catalog by capability (ground or air, shields or none),
//! never by name, so the tests do not depend on balance.

#![allow(dead_code, clippy::expect_used)]

use sim_core::catalog::SpellCatalog;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilitySource, EffectContext, EffectId, EffectTarget, EntityId, MoverType, PlayerId, ShapeKind,
    SimScalar, SimVec2, Tick, TickBundle, UnitTypeId,
};

/// A 64 × 64 flat world with the shipped catalogs and the scratch spell tables.
pub fn world(statuses: &str, effects: &str) -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat map");
    let mut world = sim_fixtures::world(map, 11);
    let spells = SpellCatalog::from_ron_strs(
        &format!("(statuses: [{statuses}])"),
        &format!("(effects: [{effects}])"),
    )
    .expect("invariant: the scratch spell tables are valid");
    world
        .catalog
        .set_spell_catalog(spells)
        .expect("invariant: the shipped abilities name no effect");
    world
}

/// A mobile archetype on `mover` with shields (`true`) or without.
pub fn archetype(world: &SimWorld, mover: MoverType, shields: bool) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .filter(|a| a.mover_type == mover && (a.max_shields > SimScalar::ZERO) == shields)
        .filter(|a| a.max_hp >= SimScalar::from_i32(40) && a.max_speed > SimScalar::ZERO)
        .map(|a| a.unit_type_id)
        .next()
        .expect("invariant: the shipped catalog has such a unit")
}

/// Spawns a ground unit without shields for `owner` at `(x, y)`.
pub fn unit(world: &mut SimWorld, owner: u8, x: i32, y: i32) -> EntityId {
    let kind = archetype(world, MoverType::Ground, false);
    world
        .spawn_unit(kind, PlayerId(owner), SimVec2::from_i32(x, y))
        .expect("invariant: a free cell")
}

/// The id of the named effect.
pub fn effect(world: &SimWorld, name: &str) -> EffectId {
    world
        .catalog
        .spells()
        .effect_id(name)
        .expect("invariant: a named scratch effect")
}

/// A context cast by `caster` (owner 0 unless given) at `target`.
pub fn context(world: &SimWorld, caster: Option<EntityId>, target: EffectTarget) -> EffectContext {
    let (owner, position) = caster
        .and_then(|id| world.entities.get(&id))
        .map_or((PlayerId(0), SimVec2::ZERO), |e| (e.owner, e.position));
    EffectContext {
        owner,
        caster,
        ability: AbilitySource::default(),
        origin: position,
        source_point: position,
        target,
    }
}

/// Runs the named effect with `context`.
pub fn run(world: &mut SimWorld, name: &str, context: EffectContext) {
    let root = effect(world, name);
    world.run_effect(root, context);
}

/// Steps one tick with no commands; panics on a diagnostic.
pub fn step(world: &mut SimWorld) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: Vec::new(),
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Life of `id`.
pub fn hp(world: &SimWorld, id: EntityId) -> SimScalar {
    world.entities.get(&id).map_or(SimScalar::ZERO, |e| e.hp)
}

/// A scalar from its decimal text.
pub fn scalar(text: &str) -> SimScalar {
    ron::from_str(text).expect("invariant: a number")
}
