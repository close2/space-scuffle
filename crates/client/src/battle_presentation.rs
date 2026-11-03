//! Cached, read-only battle labels and capability summaries from the loaded session.
//!
//! Player slots and colors come from scenario/catalog data; roster entries come
//! from actual entities. Paused frames reuse labels, and changed counts update
//! retained strings. Presentation never advances or mutates simulation state.

use std::fmt::Write;

use bevy::prelude::*;
use bevy_egui::egui;
use sim_types::{PlayerId, UnitTypeId};

/// Who a live battle is presented to, chosen once by the launch mode.
///
/// `Player` is read by the systems that must differ for a person playing one side. `Sandbox`
/// keeps the developer view that the inspector, GPU captures and pixel tests depend on.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BattleProfile {
    /// Developer inspector with every player's data.
    #[default]
    Sandbox,
    /// A person playing one side: HUD, fog and no inspector.
    Player,
}

/// One actual archetype count displayed for a participating owner.
#[derive(Debug)]
pub struct RosterEntry {
    /// Catalog identity, independent of ordering and display names.
    pub unit_type: UnitTypeId,
    /// Cached user-facing catalog name.
    pub name: String,
    /// Reused display string, refreshed only when count changes.
    pub label: String,
    /// Current living entity count.
    pub count: usize,
    previous_count: usize,
}

/// One scenario participant and its actual current capabilities.
#[derive(Debug)]
pub struct PlayerPresentation {
    /// Actual owning slot, which need not be contiguous.
    pub owner: PlayerId,
    /// Cached player/loadout label.
    pub label: String,
    /// Authored sRGB ownership color shared with runtime map presentation.
    pub color: egui::Color32,
    /// Sorted current and previously seen archetypes, with retained text storage.
    pub roster: Vec<RosterEntry>,
    /// Living mobile units eligible for movement/hold controls.
    pub mobile: usize,
    /// Living mobile armed units eligible for attack-move.
    pub armed: usize,
    /// Living mobile units that can overclock now, by
    /// [`can_overclock`](crate::battle_pipeline::commands::can_overclock): the roster's
    /// `Overclock` button is offered only when this is non-zero.
    pub overclock: usize,
}

/// Session-derived UI metadata and validated participant selections.
#[derive(Resource, Default)]
pub struct BattlePresentation {
    /// Human-readable map title from the real map document.
    pub map_title: String,
    /// Sorted participants from the scenario rather than fixed UI slots.
    pub players: Vec<PlayerPresentation>,
    /// Actual owner targeted by micro controls.
    pub controlled: Option<PlayerId>,
    /// Actual other participant used as a destination; no alliance claim is made.
    pub target: Option<PlayerId>,
    last_tick: Option<u32>,
    generation: Option<crate::battle_pipeline::commands::SessionGeneration>,
}

/// Refreshes cached metadata after session changes and counts after actual ticks.
///
/// A new session generation resets the participants ([`BattlePresentation::reset_players`]);
/// a new tick recounts every living entity ([`BattlePresentation::recount`]) and then rewrites
/// only the labels whose count changed ([`BattlePresentation::relabel`]).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_battle_presentation(
    session: Res<'_, crate::battle_pipeline::session::BattleSession>,
    mut presentation: ResMut<'_, BattlePresentation>,
    mut lab: ResMut<'_, crate::LabState>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let Some(world) = session.world() else {
        return;
    };
    let Some(generation) = session.commands.generation() else {
        return;
    };
    if presentation.generation != Some(generation) {
        presentation.reset_players(&session.metadata, generation);
    }
    if presentation.last_tick == Some(world.current_tick) {
        return;
    }
    if let Err(message) = presentation.recount(world) {
        reject(&mut lab, &mut exit, message);
        return;
    }
    presentation.relabel();
    presentation.last_tick = Some(world.current_tick);
}

impl BattlePresentation {
    /// Replaces the participants with the committed session's, with empty rosters, and
    /// defaults the controlled and target owners to the first two participants.
    fn reset_players(
        &mut self,
        metadata: &crate::battle_pipeline::session::CommittedBattleMetadata,
        generation: crate::battle_pipeline::commands::SessionGeneration,
    ) {
        self.map_title.clone_from(&metadata.map_title);
        self.players.clear();
        for player in &metadata.players {
            self.players.push(PlayerPresentation {
                owner: player.owner,
                label: player.label.clone(),
                color: player.color,
                roster: Vec::new(),
                mobile: 0,
                armed: 0,
                overclock: 0,
            });
        }
        self.controlled = self.players.first().map(|player| player.owner);
        self.target = self.players.get(1).map(|player| player.owner);
        self.last_tick = None;
        self.generation = Some(generation);
    }

    /// Recounts every participant's roster and capability totals from the living entities of
    /// `world`. A newly seen archetype gets a roster entry; earlier entries keep their storage.
    ///
    /// # Errors
    /// A message naming the broken invariant if a living non-neutral entity's owner is not a
    /// participant, or its archetype is not in the world's catalog. Counts are partial then.
    fn recount(&mut self, world: &sim_core::world::SimWorld) -> Result<(), &'static str> {
        for player in &mut self.players {
            player.mobile = 0;
            player.armed = 0;
            player.overclock = 0;
            for entry in &mut player.roster {
                entry.previous_count = entry.count;
                entry.count = 0;
            }
        }
        for entity in world.entities.values().filter(|entity| entity.is_alive()) {
            let Ok(player_index) = self
                .players
                .binary_search_by_key(&entity.owner, |player| player.owner)
            else {
                if entity.owner == PlayerId::NEUTRAL {
                    continue;
                }
                return Err("Battle entity owner is absent from the loaded scenario");
            };
            let Ok(archetype) = world.catalog.get(entity.unit_type_id) else {
                return Err("Battle entity archetype is absent from the loaded catalog");
            };
            let player = &mut self.players[player_index];
            let index = player.roster_index(entity.unit_type_id, &archetype.display_name);
            player.roster[index].count += 1;
            if entity.is_completed && !entity.is_loaded_in_transport() && !entity.is_immobile {
                player.mobile += 1;
                player.armed += usize::from(
                    archetype.weapon.is_some()
                        || archetype.ground_weapon.is_some()
                        || archetype.air_weapon.is_some(),
                );
                player.overclock += usize::from(crate::battle_pipeline::commands::can_overclock(
                    world, entity, archetype,
                ));
            }
        }
        Ok(())
    }

    /// Rewrites the label of every roster entry whose count changed since the last recount.
    fn relabel(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: Paused or unchanged roster labels should not allocate every UI frame.
        // What: Recount only after ticks and rewrite retained strings only on changes.
        // Trade-off: Previously seen zero-count entries retain modest per-session storage.
        for player in &mut self.players {
            for entry in &mut player.roster {
                if entry.count != entry.previous_count {
                    entry.label.clear();
                    let _ = write!(entry.label, "{} × {}", entry.count, entry.name);
                }
            }
        }
    }
}

impl PlayerPresentation {
    /// Index of `unit_type`'s roster entry, inserting an empty one in sorted position if this
    /// participant has never had that archetype. `display_name` is the catalog name.
    fn roster_index(&mut self, unit_type: UnitTypeId, display_name: &str) -> usize {
        match self
            .roster
            .binary_search_by_key(&unit_type, |entry| entry.unit_type)
        {
            Ok(index) => index,
            Err(index) => {
                self.roster.insert(
                    index,
                    RosterEntry {
                        unit_type,
                        name: display_name.to_owned(),
                        label: String::with_capacity(display_name.len() + 24),
                        count: 0,
                        previous_count: usize::MAX,
                    },
                );
                index
            }
        }
    }
}

fn reject(lab: &mut crate::LabState, exit: &mut EventWriter<'_, AppExit>, message: &str) {
    error!("{message}");
    lab.battle_failure = Some(message.to_owned());
    lab.battle_paused = true;
    exit.send(AppExit::error());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A participant whose roster holds the given `(type, name)` archetypes, never yet counted.
    fn player(archetypes: &[(u16, &str)]) -> PlayerPresentation {
        let mut player = PlayerPresentation {
            owner: PlayerId(0),
            label: String::new(),
            color: egui::Color32::WHITE,
            roster: Vec::new(),
            mobile: 0,
            armed: 0,
            overclock: 0,
        };
        for (unit_type, name) in archetypes {
            player.roster_index(UnitTypeId(*unit_type), name);
        }
        player
    }

    #[test]
    fn roster_entries_stay_sorted_by_type_and_are_reused() {
        let mut player = player(&[(7, "Seventh"), (2, "Second")]);
        assert_eq!(player.roster_index(UnitTypeId(7), "Seventh"), 1);
        assert_eq!(player.roster.len(), 2);
        let types: Vec<_> = player.roster.iter().map(|entry| entry.unit_type).collect();
        assert_eq!(types, [UnitTypeId(2), UnitTypeId(7)]);
    }

    #[test]
    fn relabel_writes_new_and_changed_counts_only() {
        let mut presentation = BattlePresentation {
            players: vec![player(&[(1, "Alpha"), (2, "Beta")])],
            ..BattlePresentation::default()
        };
        presentation.players[0].roster[0].count = 3;
        presentation.relabel();
        let labels: Vec<_> = presentation.players[0]
            .roster
            .iter()
            .map(|entry| entry.label.as_str())
            .collect();
        assert_eq!(labels, ["3 × Alpha", "0 × Beta"]);

        for entry in &mut presentation.players[0].roster {
            entry.previous_count = entry.count;
            entry.label.push_str(" (kept)");
        }
        presentation.players[0].roster[1].count = 4;
        presentation.relabel();
        let labels: Vec<_> = presentation.players[0]
            .roster
            .iter()
            .map(|entry| entry.label.as_str())
            .collect();
        assert_eq!(labels, ["3 × Alpha (kept)", "4 × Beta"]);
    }
}
