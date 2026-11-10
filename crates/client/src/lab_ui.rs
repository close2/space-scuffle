//! `bevy_egui` interactive inspector dashboard and visual lab controls.
//!
//! # Architectural Responsibilities
//! This module provides a complete interactive inspection suite with three modes:
//! 1. **Close-Up Visual Lab:** Scrutinize single-bubble materials, Voronoi damage cracks,
//!    CRT scanlines, chromatic aberration, Fresnel rims, heading chevron, and turret focal lenses.
//! 2. **Live Combat Sandbox:** Deterministic 2-army battlefield stepping `sim_core::SimWorld`
//!    at 25 Hz with `StarCraft` II unit micro (Attack-Move, Kite, Hold, Stim).
//! 3. **200-Bubble Swarm Stress Test:** High-density collision readability and frame rate stress test.
//!
//! The selected unit's portrait plays the frame its preview bubble shows: the same flipbook lanes,
//! read from the bubble's material, at the same clock time ([`portrait_frame`], plan
//! `2026-11-04-battle-flipbook-animation`, D-9).

use crate::bubble_shader::{BubbleDomeMaterial, FlipbookRegistry};
use crate::flipbook_clock::{elapsed_clock, flipbook_layer, FlipbookLanes};
use crate::scene_controller::LabPreviewBubble;
use crate::selection::HeroLineupIndex;
use crate::selection::SelectionState;
use crate::vfx::CombatVisualEvent;
use crate::CatalogResource;
use asset_baker::manifest::FlipbookAtlasMeta;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_types::commands::UnitOrder;
use sim_types::enums::{AnimState, AttributeFlags};
use sim_types::ids::PlayerId;
use sim_types::math::SimVec2;
use sim_types::snapshot::CombatEventVisual;

/// Active dashboard tab in the Visual Lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabTab {
    /// Detailed close-up shader, flipbook, and indicator inspection.
    #[default]
    VisualLab,
    /// Live 2-army deterministic `sim_core` combat sandbox.
    LiveBattle,
    /// 200-bubble swarm readability and performance stress test.
    SwarmStressTest,
}

/// A Visual Lab button that plays one combat effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabVfxTrigger {
    /// Death shatter burst.
    GlassShatter,
    /// Instant ballistic tracer.
    HitscanFire,
    /// Travelling plasma projectile.
    PlasmaBolt,
    /// Continuous straight beam.
    LaserBeam,
    /// Continuous jagged arc.
    LightningArc,
    /// Melee crescent.
    MeleeSlash,
}

impl LabVfxTrigger {
    /// Every trigger, in button order.
    pub const ALL: [Self; 6] = [
        Self::GlassShatter,
        Self::HitscanFire,
        Self::PlasmaBolt,
        Self::LaserBeam,
        Self::LightningArc,
        Self::MeleeSlash,
    ];

    /// Button caption.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::GlassShatter => "Glass Shatter",
            Self::HitscanFire => "Hitscan Fire",
            Self::PlasmaBolt => "Plasma Bolt",
            Self::LaserBeam => "Laser Beam",
            Self::LightningArc => "Lightning Arc",
            Self::MeleeSlash => "Melee Slash",
        }
    }

    /// Builds the event for this trigger. `heading_rad` orients the melee slash.
    /// Positions use the snapshot convention `[x, y, altitude]` in world units.
    #[must_use]
    pub fn event(self, heading_rad: f32, owner: PlayerId) -> CombatEventVisual {
        // On the showcase stage, centred on the lab camera's focus at the world origin.
        let origin = [-1.5, 0.0, 1.0];
        let target = [1.5, 0.8, 1.0];
        match self {
            Self::GlassShatter => CombatEventVisual::BubbleShatter {
                pos: origin,
                radius: 0.6,
                is_dome: false,
                owner,
                unit_type: sim_types::UnitTypeId::default(),
            },
            Self::HitscanFire => CombatEventVisual::HitscanTracer {
                from: origin,
                to: target,
                owner,
                source: sim_types::EffectSource::default(),
            },
            Self::PlasmaBolt => CombatEventVisual::ProjectileVisual {
                pos: origin,
                vel: [2.5, 0.7, 0.0],
                owner,
                duration_secs: Some(1.2),
                source: sim_types::EffectSource::default(),
            },
            Self::LaserBeam => CombatEventVisual::BeamVisual {
                from: origin,
                to: target,
                is_lightning: false,
                owner,
                source: sim_types::EffectSource::default(),
            },
            Self::LightningArc => CombatEventVisual::BeamVisual {
                from: origin,
                to: target,
                is_lightning: true,
                owner,
                source: sim_types::EffectSource::default(),
            },
            Self::MeleeSlash => CombatEventVisual::MeleeSlashVisual {
                pos: origin,
                angle_rad: heading_rad,
                radius: 1.0,
                owner,
                source: sim_types::EffectSource::default(),
            },
        }
    }
}

/// Central state resource for the Visual Lab inspector and sandbox controls.
#[derive(Resource)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent user-facing toggle"
)]
pub struct LabState {
    /// Currently visible tab.
    pub current_tab: LabTab,
    // --- Tab 1: Close-Up Visual Lab Parameters ---
    /// Selected unit or building archetype name.
    pub archetype_name: String,
    /// Health ratio (0.0..1.0).
    pub hp_ratio: f32,
    /// Shield energy ratio (0.0..1.0).
    pub shield_ratio: f32,
    /// Facing heading orientation in degrees (-180°..180°).
    pub heading_angle_deg: f32,
    /// Turret aim orientation in degrees (-180°..180°).
    pub aim_angle_deg: f32,
    /// Movement speed of the preview actors through the smog field, in world units per second.
    pub speed: f32,
    /// Vertical construction scanline fill (0.0..1.0).
    pub build_progress: f32,
    /// Active animation state.
    pub anim_state: AnimState,
    /// Active cloaking shimmer flag.
    pub cloaked: bool,
    /// Active stimpack flag, shown as a pulsing rim on the selected actor.
    pub speed_boosted: bool,
    /// Direction of incoming damage in degrees.
    pub hit_angle_deg: f32,
    /// Shield ripple decay timer.
    pub shield_hit_flash: f32,
    /// Hull impact flare decay timer.
    pub hull_hit_flash: f32,
    /// Whether to force the 240-frame concept video texture.
    pub use_concept_video: bool,
    // --- Modular Visual Sub-System Toggles (V2 / V4 / V5 / V6) ---
    /// Dual-light studio specular glare flag (`FLAG_GLARE`).
    pub opt_glare: bool,
    /// Iridescent thin-film soap bubble Fresnel rim flag (`FLAG_FRESNEL`).
    pub opt_fresnel: bool,
    /// Spherical inner video barrel curvature & vignette flag (`FLAG_INNER_WARP`).
    pub opt_inner_warp: bool,
    /// Voronoi spiderweb damage cracks flag (`FLAG_CRACKS`).
    pub opt_cracks: bool,
    /// CRT scanlines and chromatic aberration flag (`FLAG_CRT_GLITCH`).
    pub opt_crt_glitch: bool,
    /// Equatorial hull facing chevron indicator flag.
    pub opt_facing_chevron: bool,
    /// Weapon attack aperture focal lens flag.
    pub opt_focal_lens: bool,
    /// Tactical double ground ring, rotating brackets, and overhead bar flag.
    pub opt_selection_brackets: bool,
    /// Cross-fade between neighbouring flipbook frames flag (`FLAG_FRAME_BLEND`).
    pub opt_frame_blend: bool,
    /// Active selected hero index within the 4-hero lineup (0..4).
    pub selected_hero_index: usize,
    // --- Tab 2: Live Combat Sandbox State ---
    /// Explicit request to recreate the selected declarative battle session.
    pub battle_reset_requested: bool,
    /// Visible diagnostic from a failed battle replacement; the old session is retained.
    pub battle_failure: Option<String>,
    /// Legal base approaches resolved from the currently loaded map's player starts.
    pub battle_start_positions: std::collections::BTreeMap<PlayerId, SimVec2>,
    /// Simulation paused flag.
    pub battle_paused: bool,
    /// Simulation tick execution speed multiplier (0 = pause, 1 = 1x, 2 = 2x, etc.).
    pub simulation_speed: u32,
    /// Last received checksum formatted as hex string.
    pub last_checksum_hex: String,
    // --- Tab 3: Swarm Stress Test State ---
    /// Whether the 200-bubble swarm is spawned.
    pub swarm_active: bool,
    /// Swarm entities list.
    pub swarm_bubble_count: usize,
    // --- 3D Volumetric Smog & 10 Bouncing Bubbles State ---
    /// Whether the 10-bubble wall-bouncing showcase replaces the hero lineup. Off by default,
    /// because the lab's actor controls and unit HUD act on the lineup.
    pub smog_bouncing_active: bool,
    /// 3D volumetric smog field visibility toggle.
    pub opt_smog_volume: bool,
    /// Smog emission rate multiplier.
    pub smog_emission_rate: f32,
    /// Moving bubble wake physical influence strength.
    pub smog_wake_influence: f32,
    /// Count of currently active smog bubbles.
    pub smog_bubble_count: usize,
}

impl Default for LabState {
    fn default() -> Self {
        Self {
            current_tab: LabTab::VisualLab,
            archetype_name: String::new(),
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            heading_angle_deg: 0.0,
            aim_angle_deg: 0.0,
            speed: 2.5,
            build_progress: 1.0,
            anim_state: AnimState::Idle,
            cloaked: false,
            speed_boosted: false,
            hit_angle_deg: 45.0,
            shield_hit_flash: 0.0,
            hull_hit_flash: 0.0,
            use_concept_video: false,
            opt_glare: true,
            opt_fresnel: true,
            opt_inner_warp: true,
            opt_cracks: true,
            opt_crt_glitch: true,
            opt_facing_chevron: true,
            opt_focal_lens: true,
            opt_selection_brackets: true,
            opt_frame_blend: true,
            selected_hero_index: 0,
            battle_reset_requested: false,
            battle_failure: None,
            battle_start_positions: std::collections::BTreeMap::new(),
            battle_paused: false,
            simulation_speed: 1,
            last_checksum_hex: "0x0000000000000000".to_string(),
            swarm_active: true,
            swarm_bubble_count: 0,
            smog_bouncing_active: false,
            opt_smog_volume: true,
            smog_emission_rate: 5.0,
            smog_wake_influence: 3.4,
            smog_bubble_count: 0,
        }
    }
}

/// What the inspector's unit portrait reads: the flipbooks, Bevy's time for the Lab clock, and the
/// preview actors' materials, whose lanes the portrait plays.
#[derive(SystemParam)]
pub struct PortraitSources<'w, 's> {
    /// Loaded flipbooks and portrait strips.
    pub registry: Res<'w, FlipbookRegistry>,
    /// Bevy's time; the Lab's flipbook clock is [`elapsed_clock`] of it.
    pub time: Res<'w, Time>,
    /// The preview actors' materials.
    pub materials: Res<'w, Assets<BubbleDomeMaterial>>,
    /// The preview actors, by lineup slot.
    pub previews: Query<
        'w,
        's,
        (
            &'static HeroLineupIndex,
            &'static MeshMaterial3d<BubbleDomeMaterial>,
        ),
        With<LabPreviewBubble>,
    >,
}

impl PortraitSources<'_, '_> {
    /// The flipbook lanes of the preview actor in lineup slot `slot`, as its material holds them,
    /// or `None` before the lineup has spawned.
    fn lanes_of(&self, slot: usize) -> Option<FlipbookLanes> {
        self.previews
            .iter()
            .find(|(index, _)| index.0 == slot)
            .and_then(|(_, material)| self.materials.get(&material.0))
            .map(|material| material.uniform.flipbook_lanes())
    }
}

/// The frame the inspector portrait draws: an atlas layer and its rows of the portrait strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortraitFrame {
    /// Atlas layer shown, the layer the bubble with the same lanes shows at the same clock time.
    pub layer: u32,
    /// Top and bottom of the layer in the vertical portrait strip, as `v` texture coordinates in
    /// `[0, 1]`.
    pub rows: [f32; 2],
}

/// The portrait frame of a bubble with `lanes` at clock time `now` (clock units), in the portrait
/// strip of `meta`, which holds `meta.total_frames` layers top to bottom.
///
/// The layer is [`flipbook_layer`], so the portrait and the bubble always agree; a layer past the
/// strip (a corrupt manifest) is clamped to its last row.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "atlas layer counts are far below 2^24, exact in f32"
)]
pub fn portrait_frame(now: u32, lanes: &FlipbookLanes, meta: &FlipbookAtlasMeta) -> PortraitFrame {
    let total = meta.total_frames.max(1);
    let layer = flipbook_layer(now, lanes).min(total - 1);
    PortraitFrame {
        layer,
        rows: [
            layer as f32 / total as f32,
            (layer + 1) as f32 / total as f32,
        ],
    }
}

// PERFORMANCE RATIONALE:
// Why: Redrawing the entire bevy_egui UI layout on every frame can incur CPU string allocations.
// What: Cached archetype lists, dynamic catalog lookups, and direct in-place slider mutation.
// Trade-off: Minimal UI state allocations yielding zero frame latency spikes.
/// Primary `bevy_egui` inspector system rendering the tabbed visual lab.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
pub fn lab_ui_system(
    palette: Res<'_, crate::player_presentation::PlayerPalette>,
    owner_status: Res<'_, crate::player_presentation::PresentationStatus>,
    mut contexts: EguiContexts<'_, '_>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    portrait: PortraitSources<'_, '_>,
    catalog: Res<'_, CatalogResource>,
    mut lineup: Local<'_, crate::scene_controller::ShowcaseLineup>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
    diagnostics: Res<'_, DiagnosticsStore>,
    windows: Query<'_, '_, &Window, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<'_, AppExit>,
    mut geometry: ResMut<'_, crate::input_policy::UiGeometry>,
    mut presentation: ResMut<'_, crate::battle_presentation::BattlePresentation>,
    media: Option<Res<'_, crate::media::ClientMediaStatus>>,
    battle: (
        ResMut<'_, crate::battle_pipeline::session::BattleSession>,
        Res<'_, crate::battle_selection::BattleSelection>,
        Res<'_, crate::battle_presentation::BattleProfile>,
    ),
) {
    let (mut session, battle_selection, profile) = battle;
    lineup.refresh(&catalog.0, catalog.is_changed());

    let Some(ctx) = contexts.try_ctx_mut().cloned() else {
        // Closing the primary window removes its egui context before the final
        // Update. A live window without its required context is still an error.
        if !windows.is_empty() {
            error!("Primary game window exists without an initialized egui context");
            exit.send(AppExit::error());
        }
        return;
    };

    // The HUD replaced the inspector. It ran first, so what is available excludes its bars, and
    // that is what the camera and the pointer are given.
    if *profile == crate::battle_presentation::BattleProfile::Player {
        geometry.targets.fill(None);
        geometry.pixels_per_point = ctx.pixels_per_point();
        geometry.inspector = None;
        geometry.world = Some(ctx.available_rect());
        return;
    }

    // Render SC2-Style Bottom-Center Selected Unit Video Portrait & Status HUD
    if lab_state.current_tab == LabTab::VisualLab
        && !(lab_state.smog_bouncing_active && lab_state.opt_smog_volume)
    {
        render_selected_unit_hud(
            &ctx,
            &mut contexts,
            &mut lab_state,
            &mut selection_state,
            &portrait,
            &catalog,
            &lineup,
        );
    }

    geometry.targets.fill(None);
    geometry.pixels_per_point = ctx.pixels_per_point();
    let available = ctx.available_rect();
    let panel_width = 360.0_f32.min(available.width() * 0.45);
    let panel = egui::SidePanel::left("game_inspector")
        .exact_width(panel_width)
        .resizable(false)
        .show(&ctx, |ui| {
            ui.heading("Space Scuffle");
            render_tab_bar(ui, &mut lab_state, &mut geometry);
            ui.separator();
            let controls_blocked = render_failure_banner(
                ui,
                media.as_deref(),
                &owner_status,
                &mut lab_state,
                &mut geometry,
            );
            egui::ScrollArea::vertical()
                .id_salt("inspector_controls")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_enabled_ui(!controls_blocked, |ui| match lab_state.current_tab {
                        LabTab::VisualLab => render_visual_lab_tab(
                            ui,
                            &mut lab_state,
                            &mut combat_events,
                            &catalog,
                            &mut geometry,
                            palette.effect(),
                        ),
                        LabTab::LiveBattle => render_live_battle_tab(
                            ui,
                            &mut lab_state,
                            &mut presentation,
                            &mut geometry,
                            &mut session,
                            &battle_selection,
                        ),
                        LabTab::SwarmStressTest => {
                            render_swarm_tab(ui, &mut lab_state, &diagnostics);
                        }
                    });
                    if let Some(media) = &media {
                        ui.separator();
                        ui.collapsing("Media audit", |ui| {
                            ui.small(media.summary_text());
                        });
                    }
                });
        });
    geometry.inspector = Some(panel.response.rect);
    geometry.world = Some(ctx.available_rect());
}

/// Draws the inspector's tab selector and records each tab button as a pointer target.
fn render_tab_bar(
    ui: &mut egui::Ui,
    lab_state: &mut LabState,
    geometry: &mut crate::input_policy::UiGeometry,
) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label, target) in [
            (
                LabTab::VisualLab,
                "Visual Lab",
                crate::input_policy::UiTarget::Lab,
            ),
            (
                LabTab::LiveBattle,
                "Live Battle",
                crate::input_policy::UiTarget::Battle,
            ),
            (
                LabTab::SwarmStressTest,
                "Swarm",
                crate::input_policy::UiTarget::Swarm,
            ),
        ] {
            let response = ui.selectable_value(&mut lab_state.current_tab, tab, label);
            geometry.record(target, &response, ui.clip_rect());
        }
    });
}

/// Shows the first media, ownership or simulation failure, and offers a reset when only the
/// simulation failed.
///
/// Returns `true` when the inspector controls must be disabled: a media or ownership failure
/// cannot be repaired by resetting simulation state, and a failed simulation must be reset first.
fn render_failure_banner(
    ui: &mut egui::Ui,
    media: Option<&crate::media::ClientMediaStatus>,
    owner_status: &crate::player_presentation::PresentationStatus,
    lab_state: &mut LabState,
    geometry: &mut crate::input_policy::UiGeometry,
) -> bool {
    if let Some(message) = media
        .and_then(crate::media::ClientMediaStatus::failure_text)
        .or(owner_status.failure_text())
        .or(lab_state.battle_failure.as_deref())
    {
        egui::ScrollArea::vertical()
            .id_salt("presentation_failure")
            .max_height(96.0)
            .show(ui, |ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            });
        ui.separator();
    }
    let media_failed = media
        .and_then(crate::media::ClientMediaStatus::failure)
        .is_some()
        || owner_status.failure_text().is_some();
    let simulation_failed = lab_state.battle_failure.is_some();
    // A new world can recover a simulation failure. A missing or invalid
    // media bundle cannot be repaired by resetting simulation state.
    if simulation_failed && !media_failed {
        let reset = ui.button("Reset failed battle");
        geometry.record(crate::input_policy::UiTarget::Reset, &reset, ui.clip_rect());
        if reset.clicked() {
            lab_state.battle_reset_requested = true;
        }
    }
    media_failed || simulation_failed
}

/// Binds the inspector identity to the actual selected showcase entity.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn sync_selected_actor_identity(
    actors: Query<'_, '_, (Entity, &crate::BubbleInstance, &HeroLineupIndex)>,
    mut selection: ResMut<'_, SelectionState>,
    catalog: Res<'_, CatalogResource>,
    mut lab: ResMut<'_, LabState>,
) {
    if lab.current_tab != LabTab::VisualLab {
        return;
    }
    let selected = selection
        .selected_hero_index
        .unwrap_or(lab.selected_hero_index);
    for (entity, instance, index) in &actors {
        if index.0 == selected {
            selection.select(entity, Some(instance.0.entity_id), Some(index.0));
            if let Ok(archetype) = catalog.get(instance.0.unit_type) {
                if lab.archetype_name != archetype.slug {
                    lab.archetype_name.clone_from(&archetype.slug);
                }
            }
            break;
        }
    }
}

/// Draws the Visual Lab tab, one section after another, separated by rules.
fn render_visual_lab_tab(
    ui: &mut egui::Ui,
    state: &mut LabState,
    events: &mut EventWriter<'_, CombatVisualEvent>,
    catalog: &UnitCatalog,
    geometry: &mut crate::input_policy::UiGeometry,
    effect_owner: PlayerId,
) {
    render_smog_section(ui, state);
    ui.separator();
    render_selected_actor_section(ui, state, catalog);
    ui.separator();
    render_damage_section(ui, state, geometry);
    ui.separator();
    render_orientation_section(ui, state);
    ui.separator();
    render_action_state_section(ui, state);
    ui.separator();
    render_visual_subsystems_section(ui, state);
    ui.separator();
    render_vfx_trigger_section(ui, state, events, effect_owner);
}

/// Smog field and wall-bouncing showcase controls.
fn render_smog_section(ui: &mut egui::Ui, state: &mut LabState) {
    ui.heading("3D Smog & 10 Bouncing Bubbles");
    ui.checkbox(&mut state.opt_smog_volume, "3D Volumetric Smog Field");
    ui.checkbox(&mut state.smog_bouncing_active, "10-Bubble Wall Bouncing");
    ui.add(egui::Slider::new(&mut state.smog_emission_rate, 0.0..=5.0).text("Emission Rate"));
    ui.add(egui::Slider::new(&mut state.smog_wake_influence, 0.0..=5.0).text("Wake Influence"));
    ui.label(format!("Active Smog Bubbles: {}", state.smog_bubble_count));
}

/// Name of the inspected actor and the concept-video override.
fn render_selected_actor_section(ui: &mut egui::Ui, state: &mut LabState, catalog: &UnitCatalog) {
    ui.heading("Selected actor");
    if let Some(actor) = catalog.get_by_slug(&state.archetype_name) {
        ui.strong(&actor.display_name);
    } else {
        ui.label("Select a visible showcase actor.");
    }

    ui.checkbox(
        &mut state.use_concept_video,
        "Force Concept Video (240 frames)",
    );
}

/// Health, shield and construction sliders, with the damage stage the health ratio selects.
fn render_damage_section(
    ui: &mut egui::Ui,
    state: &mut LabState,
    geometry: &mut crate::input_policy::UiGeometry,
) {
    ui.heading("Diegetic Damage & Status");
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::Slider::new(&mut state.hp_ratio, 0.0..=1.0)
                .show_value(false)
                .text("HP Ratio"),
        );
        let health = ui.add(
            egui::DragValue::new(&mut state.hp_ratio)
                .range(0.0..=1.0)
                .speed(0.01)
                .fixed_decimals(2),
        );
        geometry.record(
            crate::input_policy::UiTarget::Health,
            &health,
            ui.clip_rect(),
        );
    });
    if state.hp_ratio < 0.35 {
        ui.colored_label(
            egui::Color32::RED,
            "Critical Damage: Red Arcing & CRT Glitch Active",
        );
    } else if state.hp_ratio < 0.75 {
        ui.colored_label(
            egui::Color32::GOLD,
            "Moderate Damage: Voronoi Glass Cracks Active",
        );
    } else {
        ui.colored_label(egui::Color32::GREEN, "Healthy: Pristine Glossy Glass");
    }

    ui.add(egui::Slider::new(&mut state.shield_ratio, 0.0..=1.0).text("Shield Energy"));
    ui.add(egui::Slider::new(&mut state.build_progress, 0.0..=1.0).text("Construction Progress"));
}

/// Hull facing, turret aim (both in degrees) and translation speed (world units per second).
fn render_orientation_section(ui: &mut egui::Ui, state: &mut LabState) {
    ui.heading("Orientation & Turret Tracking");
    ui.add(
        egui::Slider::new(&mut state.heading_angle_deg, -180.0..=180.0)
            .text("Hull Facing (Chevron)"),
    );
    ui.add(
        egui::Slider::new(&mut state.aim_angle_deg, -180.0..=180.0).text("Turret Aim (Focal Lens)"),
    );
    ui.add(egui::Slider::new(&mut state.speed, 0.0..=8.0).text("Translation Speed"));
}

/// Animation state selector and the cloak and stim toggles.
fn render_action_state_section(ui: &mut egui::Ui, state: &mut LabState) {
    ui.heading("Action State");
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.anim_state, AnimState::Idle, "Idle");
        ui.selectable_value(&mut state.anim_state, AnimState::Walk, "Walk");
        ui.selectable_value(&mut state.anim_state, AnimState::Attack, "Attack");
        ui.selectable_value(&mut state.anim_state, AnimState::Die, "Die");
        ui.selectable_value(&mut state.anim_state, AnimState::Constructing, "Construct");
    });

    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.cloaked, "Cloaked (Refraction)");
        ui.checkbox(&mut state.speed_boosted, "Stimpack (Boost Rim)");
    });
}

/// Toggles for each modular bubble shader sub-system.
fn render_visual_subsystems_section(ui: &mut egui::Ui, state: &mut LabState) {
    ui.heading("Modular Visual Sub-Systems (V2 / V4 / V5 / V6)");
    ui.checkbox(&mut state.opt_glare, "Glass Glare & Specular Reflection");
    ui.checkbox(&mut state.opt_fresnel, "Iridescent Fresnel Rim");
    ui.checkbox(&mut state.opt_inner_warp, "Spherical Inner Video Warp");
    ui.checkbox(
        &mut state.opt_facing_chevron,
        "Direct-on-Sphere Facing Stripe",
    );
    ui.checkbox(
        &mut state.opt_focal_lens,
        "Direct-on-Sphere Attack Aperture",
    );
    ui.checkbox(&mut state.opt_cracks, "Voronoi Damage Cracks");
    ui.checkbox(&mut state.opt_crt_glitch, "CRT Scanlines & Glitch");
    ui.checkbox(
        &mut state.opt_selection_brackets,
        "SDF Selection Reticle & Overhead Bar",
    );
    ui.checkbox(&mut state.opt_frame_blend, "Flipbook Frame Blending");
}

/// Hit-flash surges and one button per [`LabVfxTrigger`], sent as owned by `effect_owner`.
fn render_vfx_trigger_section(
    ui: &mut egui::Ui,
    state: &mut LabState,
    events: &mut EventWriter<'_, CombatVisualEvent>,
    effect_owner: PlayerId,
) {
    ui.heading("Combat VFX Triggers");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Shield Ripple Surge").clicked() {
            state.shield_hit_flash = 1.0;
        }
        if ui.button("Hull Flare Surge").clicked() {
            state.hull_hit_flash = 1.0;
        }
    });

    ui.horizontal_wrapped(|ui| {
        for trigger in LabVfxTrigger::ALL {
            if ui.button(trigger.label()).clicked() {
                events.send(CombatVisualEvent(
                    trigger.event(state.heading_angle_deg.to_radians(), effect_owner),
                ));
            }
        }
    });
}

/// Draws the Live Battle tab: roster, playback, orders, reset and diagnostics, in that order.
fn render_live_battle_tab(
    ui: &mut egui::Ui,
    state: &mut LabState,
    presentation: &mut crate::battle_presentation::BattlePresentation,
    geometry: &mut crate::input_policy::UiGeometry,
    session: &mut crate::battle_pipeline::session::BattleSession,
    selection: &crate::battle_selection::BattleSelection,
) {
    if !session.is_ready() {
        ui.disable();
    }
    render_battle_roster(ui, presentation, session);
    ui.separator();
    render_battle_playback(ui, state, geometry);
    ui.separator();
    ui.heading("Orders");
    match (selection.inspected(), selection.ids().len()) {
        (Some(_), _) => ui.label("Selected: one unit of another player"),
        (None, 1) => ui.label("Selected: 1 unit"),
        (None, count) => ui.label(format!("Selected: {count} units")),
    };
    ui.small(crate::input_bindings::BATTLE_CONTROLS_HELP);
    render_participant_pickers(ui, presentation);
    render_battle_order_buttons(ui, state, presentation, session);
    ui.separator();
    let reset = ui.button("Reset battle");
    geometry.record(crate::input_policy::UiTarget::Reset, &reset, ui.clip_rect());
    if reset.clicked() {
        state.battle_reset_requested = true;
    }
    ui.label(session.commands.status());
    ui.label(&session.outcome_status);
    ui.collapsing("Diagnostics", |ui| {
        ui.label(&state.last_checksum_hex);
        let clock = session.clock.diagnostics();
        ui.label(format!(
            "Scheduling time discarded: {:.3}s total",
            clock.discarded_total.as_secs_f64()
        ));
    });
}

/// Map title, each participant's live roster, and the published tick.
fn render_battle_roster(
    ui: &mut egui::Ui,
    presentation: &crate::battle_presentation::BattlePresentation,
    session: &crate::battle_pipeline::session::BattleSession,
) {
    ui.heading(&presentation.map_title);
    for player in &presentation.players {
        ui.colored_label(player.color, &player.label);
        for entry in &player.roster {
            if entry.count > 0 {
                ui.label(&entry.label);
            }
        }
        ui.add_space(4.0);
    }
    if let Some(published) = session.published() {
        ui.label(format!(
            "Tick {} · {} active entities",
            published.tick, published.entity_count
        ));
    } else {
        ui.label("Preparing the selected battle…");
    }
}

/// Pause toggle and the simulation speed multiplier.
fn render_battle_playback(
    ui: &mut egui::Ui,
    state: &mut LabState,
    geometry: &mut crate::input_policy::UiGeometry,
) {
    ui.horizontal_wrapped(|ui| {
        let pause = ui.button(if state.battle_paused {
            "Resume"
        } else {
            "Pause"
        });
        geometry.record(crate::input_policy::UiTarget::Pause, &pause, ui.clip_rect());
        if pause.clicked() {
            state.battle_paused = !state.battle_paused;
        }
        ui.add(egui::Slider::new(&mut state.simulation_speed, 1..=4).text("Speed"));
    });
}

/// Chooses the controlled participant and the destination participant.
///
/// The destination is never the controlled participant: when they coincide, it moves to the
/// first other participant.
fn render_participant_pickers(
    ui: &mut egui::Ui,
    presentation: &mut crate::battle_presentation::BattlePresentation,
) {
    let selected_name = presentation
        .players
        .iter()
        .find(|player| Some(player.owner) == presentation.controlled)
        .map_or("No participant", |player| player.label.as_str());
    ui.label("Control");
    egui::ComboBox::from_id_salt("controlled_participant")
        .width(ui.available_width())
        .truncate()
        .selected_text(selected_name)
        .show_ui(ui, |ui| {
            for player in &presentation.players {
                ui.selectable_value(
                    &mut presentation.controlled,
                    Some(player.owner),
                    &player.label,
                );
            }
        });
    if presentation.target == presentation.controlled {
        presentation.target = presentation
            .players
            .iter()
            .find(|player| Some(player.owner) != presentation.controlled)
            .map(|player| player.owner);
    }
    let target_name = presentation
        .players
        .iter()
        .find(|player| Some(player.owner) == presentation.target)
        .map_or("No other participant", |player| player.label.as_str());
    ui.label("Destination");
    egui::ComboBox::from_id_salt("destination_participant")
        .width(ui.available_width())
        .truncate()
        .selected_text(target_name)
        .show_ui(ui, |ui| {
            for player in &presentation.players {
                if Some(player.owner) != presentation.controlled {
                    ui.selectable_value(
                        &mut presentation.target,
                        Some(player.owner),
                        &player.label,
                    );
                }
            }
        });
}

/// Roster-wide orders for the controlled participant: attack the destination's base, return to
/// its own base, hold, and overclock when any unit can.
fn render_battle_order_buttons(
    ui: &mut egui::Ui,
    state: &LabState,
    presentation: &crate::battle_presentation::BattlePresentation,
    session: &mut crate::battle_pipeline::session::BattleSession,
) {
    if let Some(player) = presentation
        .players
        .iter()
        .find(|player| Some(player.owner) == presentation.controlled)
    {
        ui.vertical(|ui| {
            if ui
                .add_enabled(
                    player.armed > 0 && presentation.target.is_some(),
                    egui::Button::new("Attack destination"),
                )
                .clicked()
            {
                if let Some(target) = presentation
                    .target
                    .and_then(|owner| state.battle_start_positions.get(&owner).copied())
                {
                    issue_sandbox_command(session, player.owner, UnitOrder::AttackMove { target });
                }
            }
            if ui
                .add_enabled(player.mobile > 0, egui::Button::new("Return to base"))
                .clicked()
            {
                if let Some(target) = state.battle_start_positions.get(&player.owner).copied() {
                    issue_sandbox_command(session, player.owner, UnitOrder::Move { target });
                }
            }
            if ui
                .add_enabled(player.mobile > 0, egui::Button::new("Hold position"))
                .clicked()
            {
                issue_sandbox_command(session, player.owner, UnitOrder::HoldPosition);
            }
            if player.overclock > 0 && ui.button("Overclock").clicked() {
                issue_sandbox_command(session, player.owner, UnitOrder::UseStim);
            }
        });
    }
}

fn issue_sandbox_command(
    session: &mut crate::battle_pipeline::session::BattleSession,
    player_id: PlayerId,
    order: UnitOrder,
) {
    let (world, queue) = session.world_and_commands();
    let result = world
        .ok_or(crate::battle_pipeline::commands::SubmissionError::NotReady)
        .and_then(|world| queue.submit_roster_order(world, player_id, order, false));
    if let Err(error) = result {
        queue.reject(&error);
    }
}

fn render_swarm_tab(ui: &mut egui::Ui, state: &mut LabState, diagnostics: &DiagnosticsStore) {
    ui.heading("200-Bubble Swarm Stress Test");
    ui.label(
        "Evaluates rendering readability and GPU uniform throughput with 200 colliding units.",
    );

    if let Some(fps) = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS) {
        if let Some(val) = fps.smoothed() {
            ui.label(format!("FPS: {val:.1}"));
        }
    }

    ui.label(format!(
        "Active Swarm Bubbles: {}",
        state.swarm_bubble_count
    ));
    ui.label("Alternate ownership palettes are not available.");

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Spawn 200-Bubble Swarm").clicked() {
            state.swarm_active = true;
        }
        if ui.button("Clear Swarm").clicked() {
            state.swarm_active = false;
        }
    });
}

/// Renders the `StarCraft` II style bottom-center unit portrait, faction lore, vitals, and weapon HUD.
fn render_selected_unit_hud(
    ctx: &egui::Context,
    contexts: &mut EguiContexts<'_, '_>,
    lab_state: &mut LabState,
    selection_state: &mut SelectionState,
    portrait: &PortraitSources<'_, '_>,
    catalog: &UnitCatalog,
    lineup: &crate::scene_controller::ShowcaseLineup,
) {
    let active_hero_idx = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(lineup.len().saturating_sub(1));

    let arch = lineup
        .ids()
        .get(active_hero_idx)
        .and_then(|id| catalog.get(*id).ok());

    egui::TopBottomPanel::bottom("selected_unit_hud")
        .exact_height(240.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                render_unit_portrait(ui, contexts, lab_state, portrait, arch, active_hero_idx);

                ui.add_space(14.0);

                // Right unit lore, faction badge, live vitals, weapon stats, and quick-select buttons
                ui.vertical(|ui| {
                    if let Some(arch) = arch {
                        render_unit_identity_row(ui, lab_state, catalog, arch);
                        render_unit_role_row(ui, arch);
                        ui.separator();
                        render_unit_combat_rows(ui, lab_state, arch);
                        ui.separator();
                        render_lineup_buttons(
                            ui,
                            lab_state,
                            selection_state,
                            catalog,
                            lineup,
                            active_hero_idx,
                        );
                    }
                });
            });
        });
}

/// Draws the 200×200 animated video portrait: the concept video when forced, otherwise the
/// selected archetype's portrait flipbook, showing the frame the preview actor in lineup slot
/// `slot` shows ([`portrait_frame`]).
///
/// The inspector draws before the preview is synchronised, so on the frame the Lab's state
/// changes the portrait still plays the previous state's lanes; from the next frame on it matches.
fn render_unit_portrait(
    ui: &mut egui::Ui,
    contexts: &mut EguiContexts<'_, '_>,
    lab_state: &LabState,
    portrait: &PortraitSources<'_, '_>,
    arch: Option<&UnitArchetype>,
    slot: usize,
) {
    let registry = &portrait.registry;
    let (portrait_handle, meta) = if lab_state.use_concept_video {
        (
            registry.get_portrait_by_name("concept_video"),
            registry.get_meta_by_name("concept_video"),
        )
    } else if let Some(arch) = arch {
        (
            registry.get_portrait_by_id(arch.unit_type_id),
            registry.get_meta(arch.unit_type_id),
        )
    } else {
        (None, None)
    };

    ui.allocate_ui_with_layout(
        egui::vec2(200.0, 200.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            if let (Some(handle), Some(meta)) = (portrait_handle, meta) {
                let tex_id = contexts.add_image(handle.clone());
                // The lineup spawns on the frame after the first one; nothing to draw before it.
                let Some(lanes) = portrait.lanes_of(slot) else {
                    return;
                };
                let frame = portrait_frame(elapsed_clock(portrait.time.elapsed()), &lanes, meta);
                let [v0, v1] = frame.rows;

                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        tex_id,
                        egui::vec2(200.0, 200.0),
                    ))
                    .uv(egui::Rect::from_min_max(
                        egui::pos2(0.0, v0),
                        egui::pos2(1.0, v1),
                    )),
                );
            } else {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "Portrait unavailable. See the media diagnostic.",
                );
            }
        },
    );
}

/// The faction badge of `arch`: its faction's display name and sRGB accent colour, both read
/// from `assets/catalog/factions.ron` through the catalog. `None` for an archetype whose faction
/// the catalog does not define, which only programmatic registration can produce.
fn faction_badge<'a>(
    catalog: &'a UnitCatalog,
    arch: &UnitArchetype,
) -> Option<(&'a str, egui::Color32)> {
    let faction = catalog.factions().faction(arch.faction)?;
    let (red, green, blue) = faction.accent_srgb;
    Some((
        faction.display_name.as_str(),
        egui::Color32::from_rgb(red, green, blue),
    ))
}

/// Faction badge and names on the left; health and shields, scaled by the lab's ratios, on the
/// right.
fn render_unit_identity_row(
    ui: &mut egui::Ui,
    lab_state: &LabState,
    catalog: &UnitCatalog,
    arch: &UnitArchetype,
) {
    let badge = faction_badge(catalog, arch);

    ui.horizontal_wrapped(|ui| {
        if let Some((faction_name, faction_color)) = badge {
            ui.colored_label(faction_color, format!("[ {faction_name} ]"));
        }
        ui.strong(format!("{} — {}", arch.display_name, arch.name));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let hp_val = (arch.max_hp.to_f32() * lab_state.hp_ratio).round();
            let max_hp = arch.max_hp.to_f32();
            ui.colored_label(
                egui::Color32::from_rgb(80, 240, 100),
                format!("HP: {hp_val:.0} / {max_hp:.0}"),
            );

            if arch.max_shields.to_f32() > 0.0 {
                let shield_val = (arch.max_shields.to_f32() * lab_state.shield_ratio).round();
                let max_shield = arch.max_shields.to_f32();
                ui.colored_label(
                    egui::Color32::from_rgb(50, 200, 255),
                    format!("Shield: {shield_val:.0} / {max_shield:.0}"),
                );
            } else {
                ui.colored_label(egui::Color32::GRAY, "Shield: --");
            }
        });
    });
}

/// Role text and one badge per attribute flag the archetype carries.
fn render_unit_role_row(ui: &mut egui::Ui, arch: &UnitArchetype) {
    ui.horizontal_wrapped(|ui| {
        let role_str = if arch.lore_description.is_empty() {
            "Tactical Combatant"
        } else {
            arch.lore_description.as_str()
        };
        ui.label(format!("Role: {role_str}"));
        ui.label("| Tags:");
        if arch.attributes.contains(AttributeFlags::LIGHT) {
            ui.colored_label(egui::Color32::LIGHT_BLUE, "[ LIGHT ]");
        }
        if arch.attributes.contains(AttributeFlags::ARMORED) {
            ui.colored_label(egui::Color32::GOLD, "[ ARMORED ]");
        }
        if arch.attributes.contains(AttributeFlags::BIOLOGICAL) {
            ui.colored_label(egui::Color32::GREEN, "[ BIOLOGICAL ]");
        }
        if arch.attributes.contains(AttributeFlags::MECHANICAL) {
            ui.colored_label(egui::Color32::LIGHT_GRAY, "[ MECHANICAL ]");
        }
        if arch.attributes.contains(AttributeFlags::STRUCTURE) {
            ui.colored_label(egui::Color32::KHAKI, "[ STRUCTURE ]");
        }
    });
}

/// Weapon envelope (damage, range in world units, period in seconds and ticks) and the mobility
/// and armour row.
fn render_unit_combat_rows(ui: &mut egui::Ui, lab_state: &LabState, arch: &UnitArchetype) {
    if let Some(ref weapon) = arch.weapon {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Weapon: {}", weapon.name));
            ui.label(format!("Damage: {:.1}", weapon.base_damage.to_f32()));
            ui.label(format!("Range: {:.1} u", weapon.max_range.to_f32()));
            ui.label(format!(
                "Period: {:.2}s ({} ticks)",
                f32::from(weapon.period_ticks)
                    * crate::battle_pipeline::clock::SIM_TICK_PERIOD.as_secs_f32(),
                weapon.period_ticks
            ));
            ui.label(format!("Push Priority: {}", arch.push_priority));
        });
    } else {
        let label = if arch.attributes.contains(AttributeFlags::STRUCTURE) {
            "Structure: Heavy Armor Matrix | Construction Nexus"
        } else {
            "Unarmed Tactical Unit"
        };
        ui.label(label);
    }

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Mover: {:?}", arch.mover_type));
        ui.label(format!("Life Armor: {:.0}", arch.life_armor.to_f32()));
        ui.label(format!("Shield Armor: {:.0}", arch.shield_armor.to_f32()));
        ui.label(format!("Aim Angle: {:.1}°", lab_state.aim_angle_deg));
        ui.label(format!("Speed: {:.2} u/s", lab_state.speed));
    });
}

/// Quick-select buttons for the hero showcase lineup; a click selects that lineup index in both
/// the lab state and the selection state.
fn render_lineup_buttons(
    ui: &mut egui::Ui,
    lab_state: &mut LabState,
    selection_state: &mut SelectionState,
    catalog: &UnitCatalog,
    lineup: &crate::scene_controller::ShowcaseLineup,
    active_hero_idx: usize,
) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("Hero Showcase Lineup:");
        for (idx, hero_arch) in lineup.archetypes(catalog).enumerate() {
            let label = format!("{}. {}", idx + 1, hero_arch.display_name);
            if ui.selectable_label(active_hero_idx == idx, label).clicked() {
                lab_state.selected_hero_index = idx;
                selection_state.selected_hero_index = Some(idx);
                lab_state.archetype_name.clone_from(&hero_arch.slug);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 9 WP12: the badge now comes from `factions.ron`. Players must see what they saw
    /// when the names and colours were a `match` here, so the shipped data is pinned to them.
    #[test]
    fn shipped_faction_badges_are_unchanged_and_come_from_the_catalog() {
        let catalog = sim_fixtures::shipped_catalogs().units;
        let mut badges: Vec<(&str, egui::Color32)> = catalog
            .archetypes()
            .filter_map(|arch| faction_badge(&catalog, arch))
            .collect();
        assert_eq!(
            badges.len(),
            catalog.archetypes().count(),
            "every shipped archetype has a badge"
        );
        badges.sort_by_key(|(name, _)| *name);
        badges.dedup();
        assert_eq!(
            badges,
            [
                (
                    "Automaton Directorate",
                    egui::Color32::from_rgb(60, 150, 255)
                ),
                ("Cosmic Enclave", egui::Color32::from_rgb(50, 230, 230)),
                ("Sylvan Swarm", egui::Color32::from_rgb(255, 120, 40)),
            ]
        );
    }

    #[test]
    fn an_archetype_whose_faction_is_not_in_the_catalog_has_no_badge() -> Result<(), &'static str> {
        let catalog = sim_fixtures::shipped_catalogs().units;
        let mut arch = catalog
            .archetypes()
            .next()
            .cloned()
            .ok_or("the shipped catalog has archetypes")?;
        arch.faction = (0..=u8::MAX)
            .map(sim_types::FactionId)
            .find(|id| catalog.factions().faction(*id).is_none())
            .ok_or("fewer than 256 factions are shipped")?;
        assert_eq!(faction_badge(&catalog, &arch), None);
        Ok(())
    }
}
