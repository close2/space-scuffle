//! Game scene ownership for global environment and visible light hierarchies.
//!
//! The Lab stage globals are captured after Startup. A battle environment is
//! attached only to its successfully committed root, so failed replacements cannot
//! change the visible scene. Visibility is applied before inherited propagation.

use asset_baker::TerrainTheme;
use bevy::prelude::*;
use bevy::render::view::VisibilitySystems;

use super::BattleSceneRoot;
use crate::lab_stage::LabStageMarker;
use crate::{LabState, LabTab};

#[derive(Clone)]
struct EnvironmentValues {
    ambient: AmbientLight,
    clear: ClearColor,
}

/// Environment belonging to one fully prepared battle root, never a candidate.
#[derive(Component)]
pub(super) struct CommittedBattleEnvironment(EnvironmentValues);

impl CommittedBattleEnvironment {
    pub(super) fn from_theme(theme: &TerrainTheme) -> Self {
        let (ambient, clear) = crate::terrain::theme_gpu::theme_environment(theme);
        Self(EnvironmentValues { ambient, clear })
    }
}

#[derive(Resource, Default)]
struct SceneEnvironmentState {
    baseline: Option<EnvironmentValues>,
    active_battle: Option<Entity>,
    failed: bool,
}

/// Applies game scene resources and root visibility as one presentation stage.
///
/// Startup captures the Lab stage values. PostUpdate runs after UI/scene commands
/// and before visibility propagation; standalone terrain/editor setup is unchanged.
pub struct SceneEnvironmentPlugin;

impl Plugin for SceneEnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClearColor>()
            .init_resource::<AmbientLight>()
            .init_resource::<SceneEnvironmentState>()
            .add_systems(PostStartup, capture_stage_environment)
            .add_systems(
                PostUpdate,
                apply_scene_environment.before(VisibilitySystems::VisibilityPropagate),
            );
    }
}

fn capture_stage_environment(
    ambient: Res<'_, AmbientLight>,
    clear: Res<'_, ClearColor>,
    mut state: ResMut<'_, SceneEnvironmentState>,
) {
    if state.baseline.is_none() {
        state.baseline = Some(EnvironmentValues {
            ambient: ambient.clone(),
            clear: clear.clone(),
        });
    }
}

#[derive(Debug, thiserror::Error)]
enum SceneEnvironmentError {
    #[error("Game scene environment was not captured after Startup")]
    MissingBaseline,
    #[error("Game scene has multiple committed battle environments")]
    MultipleBattles,
}

type VisibleScenes<'w, 's> = (
    Query<'w, 's, (Entity, &'static mut Visibility), With<BattleSceneRoot>>,
    Query<'w, 's, &'static mut Visibility, With<LabStageMarker>>,
);

fn apply_scene_environment(
    mut lab: ResMut<'_, LabState>,
    mut state: ResMut<'_, SceneEnvironmentState>,
    committed: Query<'_, '_, (Entity, &CommittedBattleEnvironment), With<BattleSceneRoot>>,
    mut visible: ParamSet<'_, '_, VisibleScenes<'_, '_>>,
    mut ambient: ResMut<'_, AmbientLight>,
    mut clear: ResMut<'_, ClearColor>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if state.failed {
        return;
    }
    let mut roots = committed.iter();
    let root = roots.next();
    let failure = if state.baseline.is_none() {
        Some(SceneEnvironmentError::MissingBaseline)
    } else if roots.next().is_some() {
        Some(SceneEnvironmentError::MultipleBattles)
    } else {
        None
    };
    if let Some(failure) = failure {
        error!("{failure}");
        lab.battle_failure = Some(failure.to_string());
        lab.battle_paused = true;
        state.failed = true;
        exit.send(AppExit::error());
        return;
    }
    let active = if lab.current_tab == LabTab::LiveBattle {
        // First tab selection can precede its scene commit. Keep the actual prior
        // stage presentation until a root exists; never read a candidate theme.
        root
    } else {
        None
    };
    let values = active
        .map(|(_, environment)| &environment.0)
        .or(state.baseline.as_ref());
    if let Some(values) = values {
        // These are copied authored values, not calculated measurements: preserve
        // exact state and avoid emitting false resource changes on steady frames.
        if ambient.color != values.ambient.color
            || ambient.brightness.to_bits() != values.ambient.brightness.to_bits()
        {
            *ambient = values.ambient.clone();
        }
        if clear.0 != values.clear.0 {
            *clear = values.clear.clone();
        }
    }
    let active_root = active.map(|(entity, _)| entity);
    if state.active_battle != active_root {
        state.active_battle = active_root;
    }
    for (entity, mut visibility) in &mut visible.p0() {
        let desired = if Some(entity) == active_root {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != desired {
            *visibility = desired;
        }
    }
    for mut visibility in &mut visible.p1() {
        let desired = if active_root.is_some() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        if *visibility != desired {
            *visibility = desired;
        }
    }
}

#[cfg(test)]
mod tests;
