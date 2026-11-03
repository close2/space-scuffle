//! The live preview of a structure being placed: one ghost on the ground, and its verdict.
//!
//! # Responsibility
//! Owns [`PlacementPreview`] and the pooled ghost entity. While a
//! [`CommandMode::Place`] is armed, [`update_placement_preview`] finds the site under the pointer
//! and asks the simulation whether the player knows of a reason it cannot be used.
//!
//! # How it fits
//! Issuing belongs to [`CommandMode::order_at`]. The ghost is only sight, but both go through
//! [`placement_cell`] and `SimWorld::preview_site`, so what the player sees is what the click does.
//! This is the HUD's one read of the world outside the model (D-2), because its input is a cursor
//! cell and not a tick.
//!
//! # Invariants
//! - The ghost is refused only for what the viewer knows, so it cannot reveal a hidden structure.
//! - With the pointer over a panel the ghost keeps its last cell, and a click there places nothing.
//! - The simulation is asked only when the site, the structure or the tick changed.
//! - One ghost is spawned at startup, so previewing spawns and allocates nothing.

use bevy::prelude::*;
use sim_types::{SiteRefusal, UnitTypeId};

use crate::battle_input::command_mode::{footprint, placement_cell, site_center, CommandMode};
use crate::battle_input::BattleView;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::status::site_reason;

/// Linear RGBA of a ghost on a site the player may use.
const LEGAL_COLOR: Color = Color::srgba(0.25, 1.0, 0.45, 0.45);

/// Linear RGBA of a ghost on a site the player knows is refused.
const REFUSED_COLOR: Color = Color::srgba(1.0, 0.2, 0.15, 0.45);

/// Height above the ground in world units, which keeps the quad out of the terrain surface.
const GHOST_GROUND_CLEARANCE: f32 = 0.08;

/// The site under the pointer while a structure is being placed.
#[derive(Resource, Debug, Default)]
pub struct PlacementPreview {
    /// Minimum corner of the site, in grid cells. `None` when the pointer finds no site.
    pub cell: Option<(u32, u32)>,
    /// Whether the player may order that site, by what the player knows.
    pub verdict: Option<Result<(), SiteRefusal>>,
    asked: Option<(UnitTypeId, (u32, u32), u32)>,
}

impl PlacementPreview {
    /// Why the site is refused, in the words of the arrival rejection, if it is.
    #[must_use]
    pub fn refusal(&self) -> Option<&'static str> {
        match self.verdict {
            Some(Err(refusal)) => Some(site_reason(refusal)),
            Some(Ok(())) | None => None,
        }
    }
}

/// Marks the pooled ghost.
#[derive(Component, Debug)]
pub struct PlacementGhost;

/// The ghost's two fixed materials.
#[derive(Resource, Debug)]
pub struct PlacementGhostAssets {
    /// Shown on a site the player may use.
    pub legal: Handle<StandardMaterial>,
    /// Shown on a site the player knows is refused.
    pub refused: Handle<StandardMaterial>,
}

/// Spawns the hidden ghost: a unit quad, scaled to a footprint when shown.
pub fn setup_placement_ghost(
    mut commands: Commands<'_, '_>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    let mut material = |base_color| {
        materials.add(StandardMaterial {
            base_color,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })
    };
    let assets = PlacementGhostAssets {
        legal: material(LEGAL_COLOR),
        refused: material(REFUSED_COLOR),
    };
    commands.spawn((
        PlacementGhost,
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(0.5)))),
        MeshMaterial3d(assets.legal.clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
    commands.insert_resource(assets);
}

type GhostQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Visibility,
        &'static mut MeshMaterial3d<StandardMaterial>,
    ),
    With<PlacementGhost>,
>;

fn hide(ghosts: &mut GhostQuery<'_, '_>) {
    for (_, mut visibility, _) in ghosts {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Moves the ghost to the site under the pointer, and judges that site.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_placement_preview(
    view: BattleView<'_, '_>,
    mode: Res<'_, CommandMode>,
    session: Res<'_, BattleSession>,
    assets: Res<'_, PlacementGhostAssets>,
    mut preview: ResMut<'_, PlacementPreview>,
    mut ghosts: GhostQuery<'_, '_>,
) {
    let (CommandMode::Place(structure), Some(world), Some(owner)) =
        (*mode, session.world(), view.owner())
    else {
        if preview.cell.is_some() || preview.verdict.is_some() {
            *preview = PlacementPreview::default();
        }
        hide(&mut ghosts);
        return;
    };
    if !view.pointer_owned() {
        return;
    }
    let Ok(archetype) = world.catalog.get(structure) else {
        return;
    };
    let cell = view
        .cursor_in_viewport()
        .and_then(|cursor| view.ray(cursor))
        .and_then(|ray| view.ground_point(ray))
        .and_then(|ground| placement_cell(world, archetype, ground.x, ground.z));
    let Some(cell) = cell else {
        preview.cell = None;
        preview.verdict = None;
        preview.asked = None;
        hide(&mut ghosts);
        return;
    };
    let question = (structure, cell, world.current_tick);
    if preview.asked == Some(question) {
        return;
    }
    let verdict = world.preview_site(owner, structure, cell.0, cell.1);
    preview.asked = Some(question);
    preview.cell = Some(cell);
    preview.verdict = Some(verdict);

    let (Some(center), Some((width, height))) =
        (site_center(world, archetype, cell), footprint(archetype))
    else {
        return;
    };
    for (mut transform, mut visibility, mut material) in &mut ghosts {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a footprint is a few cells across"
        )]
        let scale = Vec3::new(width as f32, 1.0, height as f32);
        *transform = Transform::from_translation(center + Vec3::Y * GHOST_GROUND_CLEARANCE)
            .with_scale(scale);
        let wanted = if verdict.is_ok() {
            &assets.legal
        } else {
            &assets.refused
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
        *visibility = Visibility::Inherited;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_with_no_verdict_refuses_nothing() {
        assert_eq!(PlacementPreview::default().refusal(), None);
    }

    #[test]
    fn a_legal_site_refuses_nothing() {
        let preview = PlacementPreview {
            verdict: Some(Ok(())),
            ..PlacementPreview::default()
        };
        assert_eq!(preview.refusal(), None);
    }

    #[test]
    fn a_refused_site_uses_the_words_of_the_arrival_rejection() {
        let preview = PlacementPreview {
            verdict: Some(Err(SiteRefusal::CoversResource)),
            ..PlacementPreview::default()
        };
        assert_eq!(
            preview.refusal(),
            Some(site_reason(SiteRefusal::CoversResource))
        );
    }
}
