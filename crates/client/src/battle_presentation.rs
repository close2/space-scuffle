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
    /// Living mobile units with the actual optional overclock capability.
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
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
        presentation
            .map_title
            .clone_from(&session.metadata.map_title);
        presentation.players.clear();
        for player in &session.metadata.players {
            presentation.players.push(PlayerPresentation {
                owner: player.owner,
                label: player.label.clone(),
                color: player.color,
                roster: Vec::new(),
                mobile: 0,
                armed: 0,
                overclock: 0,
            });
        }
        presentation.controlled = presentation.players.first().map(|player| player.owner);
        presentation.target = presentation.players.get(1).map(|player| player.owner);
        presentation.last_tick = None;
        presentation.generation = Some(generation);
    }
    if presentation.last_tick == Some(world.current_tick) {
        return;
    }
    for player in &mut presentation.players {
        player.mobile = 0;
        player.armed = 0;
        player.overclock = 0;
        for entry in &mut player.roster {
            entry.previous_count = entry.count;
            entry.count = 0;
        }
    }
    for entity in world.entities.values().filter(|entity| entity.is_alive()) {
        let Ok(player_index) = presentation
            .players
            .binary_search_by_key(&entity.owner, |player| player.owner)
        else {
            if entity.owner == PlayerId::NEUTRAL {
                continue;
            }
            reject(
                &mut lab,
                &mut exit,
                "Battle entity owner is absent from the loaded scenario",
            );
            return;
        };
        let Ok(archetype) = world.catalog.get(entity.unit_type_id) else {
            reject(
                &mut lab,
                &mut exit,
                "Battle entity archetype is absent from the loaded catalog",
            );
            return;
        };
        let player = &mut presentation.players[player_index];
        let index = match player
            .roster
            .binary_search_by_key(&entity.unit_type_id, |entry| entry.unit_type)
        {
            Ok(index) => index,
            Err(index) => {
                player.roster.insert(
                    index,
                    RosterEntry {
                        unit_type: entity.unit_type_id,
                        name: archetype.display_name.clone(),
                        label: String::with_capacity(archetype.display_name.len() + 24),
                        count: 0,
                        previous_count: usize::MAX,
                    },
                );
                index
            }
        };
        player.roster[index].count += 1;
        if entity.is_completed && !entity.is_loaded_in_transport() && !entity.is_immobile {
            player.mobile += 1;
            player.armed += usize::from(
                archetype.weapon.is_some()
                    || archetype.ground_weapon.is_some()
                    || archetype.air_weapon.is_some(),
            );
            player.overclock += usize::from(archetype.overclock.is_some());
        }
    }
    // PERFORMANCE RATIONALE:
    // Why: Paused or unchanged roster labels should not allocate every UI frame.
    // What: Recount only after ticks and rewrite retained strings only on changes.
    // Trade-off: Previously seen zero-count entries retain modest per-session storage.
    for player in &mut presentation.players {
        for entry in &mut player.roster {
            if entry.count != entry.previous_count {
                entry.label.clear();
                let _ = write!(entry.label, "{} × {}", entry.count, entry.name);
            }
        }
    }
    presentation.last_tick = Some(world.current_tick);
}

fn reject(lab: &mut crate::LabState, exit: &mut EventWriter<'_, AppExit>, message: &str) {
    error!("{message}");
    lab.battle_failure = Some(message.to_owned());
    lab.battle_paused = true;
    exit.send(AppExit::error());
}
