//! bevy_egui interactive inspector dashboard and visual lab controls.
//!
//! # Architectural Responsibilities
//! This module provides a complete interactive inspection suite with three modes:
//! 1. **Close-Up Visual Lab:** Scrutinize single-bubble materials, Voronoi damage cracks,
//!    CRT scanlines, chromatic aberration, Fresnel rims, heading chevron, and turret focal lenses.
//! 2. **Live Combat Sandbox:** Deterministic 2-army battlefield stepping `sim_core::SimWorld`
//!    at 25 Hz with StarCraft II unit micro (Attack-Move, Kite, Hold, Stim).
//! 3. **200-Bubble Swarm Stress Test:** High-density collision readability and frame rate stress test.

use crate::bubble_shader::FlipbookRegistry;
use crate::selection::SelectionState;
use crate::vfx::CombatVisualEvent;
use crate::CatalogResource;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use sim_core::catalog::UnitCatalog;
use sim_core::world::SimWorld;
use sim_types::commands::UnitOrder;
use sim_types::enums::{AnimState, AttributeFlags, Faction, SilhouetteClass};
use sim_types::ids::PlayerId;
use sim_types::math::SimVec2;
use sim_types::snapshot::CombatEventVisual;

/// Active dashboard tab in the Visual Lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabTab {
    /// Detailed close-up shader, flipbook, and indicator inspection.
    #[default]
    VisualLab,
    /// Live 2-army deterministic sim_core combat sandbox.
    LiveBattle,
    /// 200-bubble swarm readability and performance stress test.
    SwarmStressTest,
}

/// Central state resource for the Visual Lab inspector and sandbox controls.
#[derive(Resource)]
#[allow(clippy::struct_excessive_bools)]
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
    /// Movement speed driving mist trail wake.
    pub speed: f32,
    /// Vertical construction scanline fill (0.0..1.0).
    pub build_progress: f32,
    /// Active animation state.
    pub anim_state: AnimState,
    /// Active silhouette classification collar.
    pub silhouette: SilhouetteClass,
    /// Active cloaking shimmer flag.
    pub cloaked: bool,
    /// Active stimpack / hyper-wake particle flag.
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
    /// Dual-light studio specular glare flag (FLAG_GLARE).
    pub opt_glare: bool,
    /// Iridescent thin-film soap bubble Fresnel rim flag (FLAG_FRESNEL).
    pub opt_fresnel: bool,
    /// Spherical inner video barrel curvature & vignette flag (FLAG_INNER_WARP).
    pub opt_inner_warp: bool,
    /// Voronoi spiderweb damage cracks flag (FLAG_CRACKS).
    pub opt_cracks: bool,
    /// CRT scanlines and chromatic aberration flag (FLAG_CRT_GLITCH).
    pub opt_crt_glitch: bool,
    /// Multi-layered volumetric mist wake particles flag.
    pub opt_mist_wake: bool,
    /// Equatorial hull facing chevron indicator flag.
    pub opt_facing_chevron: bool,
    /// Weapon attack aperture focal lens flag.
    pub opt_focal_lens: bool,
    /// Tactical double ground ring, rotating brackets, and overhead bar flag.
    pub opt_selection_brackets: bool,
    /// Active selected hero index within the 4-hero lineup (0..4).
    pub selected_hero_index: usize,
    // --- Tab 2: Live Combat Sandbox State ---
    /// Authoritative deterministic simulation world.
    pub sim_world: Option<SimWorld>,
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
    /// Whether the 10-bubble wall-bouncing kinematics are active.
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
            silhouette: SilhouetteClass::LightProngs,
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
            opt_mist_wake: true,
            opt_facing_chevron: true,
            opt_focal_lens: true,
            opt_selection_brackets: true,
            selected_hero_index: 0,
            sim_world: None,
            battle_reset_requested: false,
            battle_failure: None,
            battle_start_positions: std::collections::BTreeMap::new(),
            battle_paused: false,
            simulation_speed: 1,
            last_checksum_hex: "0x0000000000000000".to_string(),
            swarm_active: true,
            swarm_bubble_count: 0,
            smog_bouncing_active: true,
            opt_smog_volume: true,
            smog_emission_rate: 5.0,
            smog_wake_influence: 3.4,
            smog_bubble_count: 0,
        }
    }
}

// PERFORMANCE RATIONALE:
// Why: Redrawing the entire bevy_egui UI layout on every frame can incur CPU string allocations.
// What: Cached archetype lists, dynamic catalog lookups, and direct in-place slider mutation.
// Trade-off: Minimal UI state allocations yielding zero frame latency spikes.
/// Primary bevy_egui inspector system rendering the tabbed visual lab.
pub fn lab_ui_system(
    palette: Res<'_, crate::player_presentation::PlayerPalette>,
    owner_status: Res<'_, crate::player_presentation::PresentationStatus>,
    mut contexts: EguiContexts<'_, '_>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    registry: Res<'_, FlipbookRegistry>,
    catalog: Res<'_, CatalogResource>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
    diagnostics: Res<'_, DiagnosticsStore>,
    windows: Query<'_, '_, &Window, With<bevy::window::PrimaryWindow>>,
    mut exit: EventWriter<'_, AppExit>,
    mut geometry: ResMut<'_, crate::input_policy::UiGeometry>,
    mut presentation: ResMut<'_, crate::battle_presentation::BattlePresentation>,
    media: Option<Res<'_, crate::media::ClientMediaStatus>>,
    mut session: ResMut<'_, crate::battle_pipeline::session::BattleSession>,
) {
    let Some(ctx) = contexts.try_ctx_mut().cloned() else {
        // Closing the primary window removes its egui context before the final
        // Update. A live window without its required context is still an error.
        if !windows.is_empty() {
            error!("Primary game window exists without an initialized egui context");
            exit.send(AppExit::error());
        }
        return;
    };

    // Render SC2-Style Bottom-Center Selected Unit Video Portrait & Status HUD
    if lab_state.current_tab == LabTab::VisualLab
        && !(lab_state.smog_bouncing_active && lab_state.opt_smog_volume)
    {
        render_selected_unit_hud(
            &ctx,
            &mut contexts,
            &mut lab_state,
            &mut selection_state,
            &registry,
            &catalog,
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
            ui.separator();
            if let Some(message) = media
                .as_ref()
                .and_then(|status| status.failure_text())
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
            let media_failed = media.as_ref().and_then(|status| status.failure()).is_some()
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
            egui::ScrollArea::vertical()
                .id_salt("inspector_controls")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_enabled_ui(!media_failed && !simulation_failed, |ui| {
                        match lab_state.current_tab {
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
                            ),
                            LabTab::SwarmStressTest => {
                                render_swarm_tab(ui, &mut lab_state, &diagnostics);
                            }
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

/// Binds the inspector identity to the actual selected showcase entity.
pub fn sync_selected_actor_identity(
    actors: Query<'_, '_, (Entity, &crate::BubbleInstance, &crate::HeroLineupIndex)>,
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

fn render_visual_lab_tab(
    ui: &mut egui::Ui,
    state: &mut LabState,
    events: &mut EventWriter<'_, CombatVisualEvent>,
    catalog: &UnitCatalog,
    geometry: &mut crate::input_policy::UiGeometry,
    effect_owner: PlayerId,
) {
    ui.heading("3D Smog & 10 Bouncing Bubbles");
    if ui
        .checkbox(&mut state.opt_smog_volume, "3D Volumetric Smog Field")
        .changed()
    {
        state.opt_mist_wake = state.opt_smog_volume;
    }
    ui.checkbox(&mut state.smog_bouncing_active, "10-Bubble Wall Bouncing");
    ui.add(egui::Slider::new(&mut state.smog_emission_rate, 0.0..=5.0).text("Emission Rate"));
    ui.add(egui::Slider::new(&mut state.smog_wake_influence, 0.0..=5.0).text("Wake Influence"));
    ui.label(format!("Active Smog Bubbles: {}", state.smog_bubble_count));

    ui.separator();
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

    ui.separator();
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

    ui.separator();
    ui.heading("Orientation & Turret Tracking");
    ui.add(
        egui::Slider::new(&mut state.heading_angle_deg, -180.0..=180.0)
            .text("Hull Facing (Chevron)"),
    );
    ui.add(
        egui::Slider::new(&mut state.aim_angle_deg, -180.0..=180.0).text("Turret Aim (Focal Lens)"),
    );
    ui.add(egui::Slider::new(&mut state.speed, 0.0..=8.0).text("Translation Speed (Mist Wake)"));

    ui.separator();
    ui.heading("Action State");
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.anim_state, AnimState::Idle, "Idle");
        ui.selectable_value(&mut state.anim_state, AnimState::Walk, "Walk");
        ui.selectable_value(&mut state.anim_state, AnimState::Attack, "Attack");
        ui.selectable_value(&mut state.anim_state, AnimState::Die, "Die");
        ui.selectable_value(&mut state.anim_state, AnimState::Constructing, "Construct");
    });

    ui.separator();
    ui.heading("Silhouette Collar");
    egui::ComboBox::from_label("Collar Mesh")
        .selected_text(format!("{:?}", state.silhouette))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut state.silhouette,
                SilhouetteClass::LightProngs,
                "LightProngs (Infantry/Scout)",
            );
            ui.selectable_value(
                &mut state.silhouette,
                SilhouetteClass::HeavyHex,
                "HeavyHex (Armored/Tank)",
            );
            ui.selectable_value(
                &mut state.silhouette,
                SilhouetteClass::CasterHalo,
                "CasterHalo (Caster/Support)",
            );
            ui.selectable_value(
                &mut state.silhouette,
                SilhouetteClass::MeleeSpikes,
                "MeleeSpikes (Brawler/Zealot)",
            );
            ui.selectable_value(
                &mut state.silhouette,
                SilhouetteClass::StructureBase,
                "StructureBase (Dome)",
            );
        });

    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.cloaked, "Cloaked (Refraction)");
        ui.checkbox(&mut state.speed_boosted, "Stimpack (Hyper Mist)");
    });

    ui.separator();
    ui.heading("Modular Visual Sub-Systems (V2 / V4 / V5 / V6)");
    ui.checkbox(&mut state.opt_glare, "Glass Glare & Specular Reflection");
    ui.checkbox(&mut state.opt_fresnel, "Iridescent Fresnel Rim");
    ui.checkbox(&mut state.opt_inner_warp, "Spherical Inner Video Warp");
    if ui
        .checkbox(&mut state.opt_mist_wake, "Volumetric FBM Mist Wake")
        .changed()
    {
        state.opt_smog_volume = state.opt_mist_wake;
    }
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

    ui.separator();
    ui.heading("Combat VFX Triggers");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Shield Ripple Surge").clicked() {
            state.shield_hit_flash = 1.0;
        }
        if ui.button("Hull Flare Surge").clicked() {
            state.hull_hit_flash = 1.0;
        }
        if ui.button("Glass Shatter").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::BubbleShatter {
                pos: [16.0, 16.0, 1.0],
                radius: 0.6,
                is_dome: false,
                owner: effect_owner,
            }));
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button("Hitscan Fire").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::HitscanTracer {
                from: [16.0, 16.0, 1.0],
                to: [20.0, 20.0, 1.0],
                owner: effect_owner,
            }));
        }
        if ui.button("Plasma Bolt").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::ProjectileVisual {
                pos: [16.0, 16.0, 1.0],
                vel: [6.0, 6.0, 0.0],
                owner: effect_owner,
                duration_secs: Some(1.2),
            }));
        }
        if ui.button("Laser Beam").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::BeamVisual {
                from: [16.0, 16.0, 1.0],
                to: [22.0, 22.0, 1.0],
                is_lightning: false,
                owner: effect_owner,
            }));
        }
        if ui.button("Lightning Arc").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::BeamVisual {
                from: [16.0, 16.0, 1.0],
                to: [22.0, 22.0, 1.0],
                is_lightning: true,
                owner: effect_owner,
            }));
        }
        if ui.button("Melee Slash").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::MeleeSlashVisual {
                pos: [16.0, 16.0, 1.0],
                angle_rad: state.heading_angle_deg.to_radians(),
                radius: 1.0,
                owner: effect_owner,
            }));
        }
    });
}

fn render_live_battle_tab(
    ui: &mut egui::Ui,
    state: &mut LabState,
    presentation: &mut crate::battle_presentation::BattlePresentation,
    geometry: &mut crate::input_policy::UiGeometry,
    session: &mut crate::battle_pipeline::session::BattleSession,
) {
    if !session.is_ready() {
        ui.disable();
    }
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
    if let Some(world) = &state.sim_world {
        ui.label(format!(
            "Tick {} · {} active entities",
            world.current_tick,
            world.entities.len()
        ));
    } else {
        ui.label("Preparing the selected battle…");
    }
    ui.separator();
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
    ui.separator();
    ui.heading("Orders");
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
                    issue_sandbox_command(
                        state,
                        &mut session.commands,
                        player.owner,
                        UnitOrder::AttackMove { target },
                    );
                }
            }
            if ui
                .add_enabled(player.mobile > 0, egui::Button::new("Return to base"))
                .clicked()
            {
                if let Some(target) = state.battle_start_positions.get(&player.owner).copied() {
                    issue_sandbox_command(
                        state,
                        &mut session.commands,
                        player.owner,
                        UnitOrder::Move { target },
                    );
                }
            }
            if ui
                .add_enabled(player.mobile > 0, egui::Button::new("Hold position"))
                .clicked()
            {
                issue_sandbox_command(
                    state,
                    &mut session.commands,
                    player.owner,
                    UnitOrder::HoldPosition,
                );
            }
            if player.overclock > 0 && ui.button("Overclock").clicked() {
                issue_sandbox_command(
                    state,
                    &mut session.commands,
                    player.owner,
                    UnitOrder::UseStim,
                );
            }
        });
    }
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

fn issue_sandbox_command(
    state: &LabState,
    queue: &mut crate::battle_pipeline::commands::BattleCommandQueue,
    player_id: PlayerId,
    order: UnitOrder,
) {
    let result = state
        .sim_world
        .as_ref()
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

/// Renders the StarCraft II style bottom-center unit portrait, faction lore, vitals, and weapon HUD.
fn render_selected_unit_hud(
    ctx: &egui::Context,
    contexts: &mut EguiContexts<'_, '_>,
    lab_state: &mut LabState,
    selection_state: &mut SelectionState,
    registry: &FlipbookRegistry,
    catalog: &UnitCatalog,
) {
    let mut heroes: Vec<_> = catalog.archetypes().filter(|a| a.showcase_hero).collect();
    heroes.sort_by_key(|a| a.showcase_order.unwrap_or(u8::MAX));

    let active_hero_idx = selection_state
        .selected_hero_index
        .unwrap_or(lab_state.selected_hero_index)
        .min(heroes.len().saturating_sub(1));

    let arch = heroes.get(active_hero_idx).copied();

    egui::TopBottomPanel::bottom("selected_unit_hud")
        .exact_height(240.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Left 200x200 live animated video portrait
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
                        if let Some(handle) = portrait_handle {
                            let tex_id = contexts.add_image(handle.clone());
                            let total_frames = meta.map_or(8, |m| m.total_frames).max(1);
                            let anim_progress = (ui.input(|i| i.time) * 1.5).fract() as f32;
                            let frame_idx =
                                (anim_progress * total_frames as f32) as u32 % total_frames;
                            let v0 = frame_idx as f32 / total_frames as f32;
                            let v1 = (frame_idx + 1) as f32 / total_frames as f32;

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

                ui.add_space(14.0);

                // Right unit lore, faction badge, live vitals, weapon stats, and quick-select buttons
                ui.vertical(|ui| {
                    if let Some(arch) = arch {
                        let (faction_name, faction_color) = match arch.faction {
                            Faction::AutomatonDirectorate => (
                                "Automaton Directorate",
                                egui::Color32::from_rgb(60, 150, 255),
                            ),
                            Faction::CosmicEnclave => {
                                ("Cosmic Enclave", egui::Color32::from_rgb(50, 230, 230))
                            }
                            Faction::SylvanSwarm => {
                                ("Sylvan Swarm", egui::Color32::from_rgb(255, 120, 40))
                            }
                        };

                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(faction_color, format!("[ {faction_name} ]"));
                            ui.strong(format!("{} — {}", arch.display_name, arch.name));

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let hp_val =
                                        (arch.max_hp.to_f32() * lab_state.hp_ratio).round();
                                    let max_hp = arch.max_hp.to_f32();
                                    ui.colored_label(
                                        egui::Color32::from_rgb(80, 240, 100),
                                        format!("HP: {hp_val:.0} / {max_hp:.0}"),
                                    );

                                    if arch.max_shields.to_f32() > 0.0 {
                                        let shield_val = (arch.max_shields.to_f32()
                                            * lab_state.shield_ratio)
                                            .round();
                                        let max_shield = arch.max_shields.to_f32();
                                        ui.colored_label(
                                            egui::Color32::from_rgb(50, 200, 255),
                                            format!("Shield: {shield_val:.0} / {max_shield:.0}"),
                                        );
                                    } else {
                                        ui.colored_label(egui::Color32::GRAY, "Shield: --");
                                    }
                                },
                            );
                        });

                        // Role and classification badges
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

                        ui.separator();

                        // Weapon specs and combat vitals
                        if let Some(ref weapon) = arch.weapon {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("Weapon: {}", weapon.name));
                                ui.label(format!("Damage: {:.1}", weapon.base_damage.to_f32()));
                                ui.label(format!("Range: {:.1} u", weapon.max_range.to_f32()));
                                ui.label(format!(
                                    "Period: {:.2}s ({} ticks)",
                                    f32::from(weapon.period_ticks)
                                        * crate::battle_pipeline::clock::SIM_TICK_PERIOD
                                            .as_secs_f32(),
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

                        ui.separator();

                        // Quick-Select Hero Showcase Lineup Buttons
                        ui.horizontal_wrapped(|ui| {
                            ui.strong("Hero Showcase Lineup:");
                            for (idx, hero_arch) in heroes.iter().enumerate() {
                                let label = format!("{}. {}", idx + 1, hero_arch.display_name);
                                if ui.selectable_label(active_hero_idx == idx, label).clicked() {
                                    lab_state.selected_hero_index = idx;
                                    selection_state.selected_hero_index = Some(idx);
                                    lab_state.archetype_name.clone_from(&hero_arch.slug);
                                }
                            }
                        });
                    }
                });
            });
        });
}

pub use crate::scene_controller::{
    step_client_simulation_system, sync_lab_preview_system, sync_live_battle_system,
    sync_swarm_stress_system, LabPreviewBubble, LiveBattleBubble, RenderEntityMap, SwarmBubble,
};
