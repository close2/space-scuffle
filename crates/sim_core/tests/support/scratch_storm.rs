//! A scratch area spell in the shape of `StarCraft` II's Psionic Storm, for tests that need a
//! damaging area without depending on the shipped spell's balance (plan
//! `2026-11-10-spellcaster-mechanics` D-12: the hazard-zone tests rewritten on the effect tree).
//!
//! The tree is the storm's: a point persistent that searches at once and then six more times
//! every 10.2 ticks, applying a status whose initial effect is damage with both responses and
//! whose periodic effect is damage without them, every 10.196 ticks for 11.9 ticks. Radius,
//! damage per pulse and whether structures are hit are parameters. Its names start with
//! `scratch_storm` so they never meet a shipped entry.

#![allow(dead_code, clippy::expect_used)]

use sim_core::SimWorld;
use sim_types::{AbilitySource, EffectContext, EffectTarget, PlayerId, SimVec2};

/// The scratch storm's parameters.
#[derive(Debug, Clone, Copy)]
pub struct ScratchStorm {
    /// Search radius in world units, as RON text (`"1.5"`).
    pub radius: &'static str,
    /// Damage of the initial and of each periodic pulse, as RON text (`"10.0"`).
    pub damage: &'static str,
    /// Whether the search admits structures as well as units.
    pub structures: bool,
}

impl ScratchStorm {
    /// The 5.0.14 storm: radius 1.5, 10 a pulse, no structures.
    pub const GAME: Self = Self {
        radius: "1.5",
        damage: "10.0",
        structures: false,
    };

    /// The `statuses.ron` and `ability_effects.ron` texts of the scratch tables.
    #[must_use]
    pub fn tables(self) -> (String, String) {
        let statuses = r#"(statuses: [(name: "scratch_storm", duration_ticks: 11.901786,
                period_ticks: Some(10.196429), initial_effect: Some("scratch_storm_initial"),
                periodic_effect: Some("scratch_storm_pulse"), presentation: [DamageOverTime])])"#
            .to_owned();
        let structures = if self.structures {
            "Include"
        } else {
            "Exclude"
        };
        let effects = format!(
            r#"(effects: [
                (name: "scratch_storm", node: Persistent((anchor: TargetPoint,
                    initial: Some(Use("scratch_storm_search")),
                    periodic: Some(Use("scratch_storm_search")),
                    period_ticks: 10.2, count: 6))),
                (name: "scratch_storm_search", node: Search((radius: {radius},
                    filter: (alliance: Any, planes: [Ground, Air], structures: {structures}),
                    child: ApplyStatus((status: "scratch_storm"))))),
                (name: "scratch_storm_initial", node: Damage((amount: {damage},
                    responses: (acquire: true, flee: true)))),
                (name: "scratch_storm_pulse", node: Damage((amount: {damage}))),
            ])"#,
            radius = self.radius,
            damage = self.damage,
        );
        (statuses, effects)
    }

    /// Installs the scratch tables, appended to the shipped ones, in `world`'s catalog.
    pub fn install(self, world: &mut SimWorld) {
        let (statuses, effects) = self.tables();
        let spells = sim_fixtures::spells_with(&statuses, &effects)
            .expect("invariant: the scratch storm tables are valid");
        world
            .catalog
            .set_spell_catalog(spells)
            .expect("invariant: the shipped abilities resolve in the merged tables");
    }
}

/// Opens the installed scratch storm at `center`, owned by `owner`, with no casting unit: the
/// walk a cast would start, so its first search runs now.
pub fn open_storm(world: &mut SimWorld, owner: PlayerId, center: SimVec2) {
    let root = world
        .catalog
        .spells()
        .effect_id("scratch_storm")
        .expect("invariant: the scratch storm is installed");
    world.run_effect(
        root,
        EffectContext {
            owner,
            caster: None,
            ability: AbilitySource::default(),
            origin: center,
            source_point: center,
            target: EffectTarget::Point(center),
        },
    );
}
