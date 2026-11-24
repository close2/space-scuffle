//! An air-plane attacker is revealed to the player it strikes (B-55).
//!
//! # Responsibility
//! Pins plan `2026-11-23-vision-and-detection` D-4 to D-6: a strike by a weapon whose attack
//! visibility is `Visible` (`StarCraft` II 5.0.14's `CEffectDamage` `Visibility Visible`, set on
//! every damage effect of air units and the Colossus) reveals its attacker to the struck unit's
//! owner for the weapon's reveal ticks (`AttackReveal`, `core` BehaviorData:237–240, 3 Normal s),
//! so that player may target it; a `Snapshot` or `Hidden` weapon reveals nothing.
//!
//! # How it fits
//! The counters live on each entity (`Entity::attack_reveal_ticks`), are set by the combat phase
//! and counted down by the vision phase; target acquisition, retaliation, casts and attack
//! dashes read them through one predicate, and `PlayerView::attack_revealed` publishes them.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by property (charter §3.3).

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, PlayerSlot, PlayerView, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// D-5: a new unit is revealed to no player, and no view lists it.
#[test]
fn a_fresh_unit_is_revealed_to_nobody() -> TestResult {
    let mut world: SimWorld = sim_fixtures::world(
        GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?,
        1,
    );
    let unit_type = world
        .catalog
        .archetypes()
        .next()
        .map(|arch| arch.unit_type_id)
        .ok_or("the shipped catalog has archetypes")?;
    let unit = world.spawn_unit(unit_type, PlayerId(0), SimVec2::from_i32(8, 8))?;
    let entity = &world.entities[&unit];
    for slot in PlayerSlot::ALL {
        assert_eq!(entity.attack_reveal_ticks[slot], 0);
        assert!(!entity.is_attack_revealed_to(PlayerId(u8::try_from(slot.index())?)));
    }
    let mut view = PlayerView::default();
    world.write_player_view(PlayerId(1), &mut view);
    assert_eq!(
        view.attack_revealed,
        Vec::new(),
        "no view lists a fresh unit"
    );
    Ok(())
}
