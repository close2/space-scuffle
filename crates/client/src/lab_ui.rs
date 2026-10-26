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
use sim_types::commands::{PlayerCommand, TickBundle, UnitOrder};
use sim_types::enums::{AnimState, AttributeFlags, Faction, SilhouetteClass};
use sim_types::ids::{EntityId, PlayerId, TeamId, Tick};
use sim_types::math::{SimScalar, SimVec2};
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
    /// Simulation paused flag.
    pub battle_paused: bool,
    /// Simulation tick execution speed multiplier (0 = pause, 1 = 1x, 2 = 2x, etc.).
    pub simulation_speed: u32,
    /// Last received checksum formatted as hex string.
    pub last_checksum_hex: String,
    /// Accumulator for fixed 25 Hz simulation ticks in the Live Battle sandbox.
    pub battle_tick_accumulator_secs: f32,
    // --- Tab 3: Swarm Stress Test State ---
    /// Whether the 200-bubble swarm is spawned.
    pub swarm_active: bool,
    /// Swarm entities list.
    pub swarm_bubble_count: usize,
    /// Team colorblind mode (Blue/Red vs Cyan/Orange).
    pub colorblind_mode: bool,
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
            battle_paused: false,
            simulation_speed: 1,
            last_checksum_hex: "0x0000000000000000".to_string(),
            battle_tick_accumulator_secs: 0.0,
            swarm_active: false,
            swarm_bubble_count: 0,
            colorblind_mode: false,
        }
    }
}

// PERFORMANCE RATIONALE:
// Why: Redrawing the entire bevy_egui UI layout on every frame can incur CPU string allocations.
// What: Cached archetype lists, dynamic catalog lookups, and direct in-place slider mutation.
// Trade-off: Minimal UI state allocations yielding zero frame latency spikes.
/// Primary bevy_egui inspector system rendering the tabbed visual lab.
pub fn lab_ui_system(
    mut contexts: EguiContexts<'_, '_>,
    mut lab_state: ResMut<'_, LabState>,
    mut selection_state: ResMut<'_, SelectionState>,
    registry: Res<'_, FlipbookRegistry>,
    catalog: Res<'_, CatalogResource>,
    mut combat_events: EventWriter<'_, CombatVisualEvent>,
    diagnostics: Res<'_, DiagnosticsStore>,
) {
    let ctx = contexts.ctx_mut().clone();

    // Render SC2-Style Bottom-Center Selected Unit Video Portrait & Status HUD
    if lab_state.current_tab == LabTab::VisualLab {
        render_selected_unit_hud(
            &ctx,
            &mut contexts,
            &mut lab_state,
            &mut selection_state,
            &registry,
            &catalog,
        );
    }

    egui::Window::new("Space Scuffle — Bubble & Dome Visual Lab")
        .default_width(380.0)
        .default_pos([16.0, 16.0])
        .show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut lab_state.current_tab,
                    LabTab::VisualLab,
                    "1. Visual Lab",
                );
                ui.selectable_value(
                    &mut lab_state.current_tab,
                    LabTab::LiveBattle,
                    "2. Live Battle",
                );
                ui.selectable_value(
                    &mut lab_state.current_tab,
                    LabTab::SwarmStressTest,
                    "3. Swarm Stress",
                );
            });

            ui.separator();

            match lab_state.current_tab {
                LabTab::VisualLab => {
                    render_visual_lab_tab(ui, &mut lab_state, &mut combat_events, &catalog);
                }
                LabTab::LiveBattle => render_live_battle_tab(ui, &mut lab_state),
                LabTab::SwarmStressTest => render_swarm_tab(ui, &mut lab_state, &diagnostics),
            }
        });
}

fn render_visual_lab_tab(
    ui: &mut egui::Ui,
    state: &mut LabState,
    events: &mut EventWriter<'_, CombatVisualEvent>,
    catalog: &UnitCatalog,
) {
    ui.heading("Archetype & Geometry");
    let archetypes: Vec<&str> = catalog.archetypes().map(|a| a.slug.as_str()).collect();

    if state.archetype_name.is_empty() {
        if let Some(first) = archetypes.first() {
            state.archetype_name = (*first).to_string();
        }
    }

    egui::ComboBox::from_label("Unit / Structure")
        .selected_text(&state.archetype_name)
        .show_ui(ui, |ui| {
            for &arch in &archetypes {
                ui.selectable_value(&mut state.archetype_name, arch.to_string(), arch);
            }
        });

    ui.checkbox(
        &mut state.use_concept_video,
        "Force Concept Video (240 frames)",
    );

    ui.separator();
    ui.heading("Diegetic Damage & Status");
    ui.add(egui::Slider::new(&mut state.hp_ratio, 0.0..=1.0).text("HP Ratio"));
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
    ui.horizontal(|ui| {
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

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.cloaked, "Cloaked (Refraction)");
        ui.checkbox(&mut state.speed_boosted, "Stimpack (Hyper Mist)");
    });

    ui.separator();
    ui.heading("Modular Visual Sub-Systems (V2 / V4 / V5 / V6)");
    ui.checkbox(&mut state.opt_glare, "Glass Glare & Specular Reflection");
    ui.checkbox(&mut state.opt_fresnel, "Iridescent Fresnel Rim");
    ui.checkbox(&mut state.opt_inner_warp, "Spherical Inner Video Warp");
    ui.checkbox(&mut state.opt_mist_wake, "Volumetric FBM Mist Wake");
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
    ui.horizontal(|ui| {
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
                team_id: TeamId(1),
            }));
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Hitscan Fire").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::HitscanTracer {
                from: [16.0, 16.0, 1.0],
                to: [20.0, 20.0, 1.0],
                team_id: TeamId(1),
            }));
        }
        if ui.button("Plasma Bolt").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::ProjectileVisual {
                pos: [16.0, 16.0, 1.0],
                vel: [6.0, 6.0, 0.0],
                team_id: TeamId(1),
                duration_secs: Some(1.2),
            }));
        }
        if ui.button("Laser Beam").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::BeamVisual {
                from: [16.0, 16.0, 1.0],
                to: [22.0, 22.0, 1.0],
                is_lightning: false,
                team_id: TeamId(1),
            }));
        }
        if ui.button("Lightning Arc").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::BeamVisual {
                from: [16.0, 16.0, 1.0],
                to: [22.0, 22.0, 1.0],
                is_lightning: true,
                team_id: TeamId(1),
            }));
        }
        if ui.button("Melee Slash").clicked() {
            events.send(CombatVisualEvent(CombatEventVisual::MeleeSlashVisual {
                pos: [16.0, 16.0, 1.0],
                angle_rad: state.heading_angle_deg.to_radians(),
                radius: 1.0,
                team_id: TeamId(1),
            }));
        }
    });
}

fn render_live_battle_tab(ui: &mut egui::Ui, state: &mut LabState) {
    ui.heading("2-Army Deterministic Sandbox");
    ui.label("Squad A (Blue): Forward Vanguard Assault");
    ui.label("Squad B (Red): Heavy Siege Armor Division");

    ui.separator();
    if let Some(ref world) = state.sim_world {
        ui.label(format!("Simulation Tick: {}", world.current_tick));
        ui.label(format!("Active Entities: {}", world.entities.len()));
        ui.label(format!(
            "Deterministic Checksum: {}",
            state.last_checksum_hex
        ));
    } else {
        ui.label("Simulation uninitialized. Click 'Initialize Sandbox' below.");
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui
            .button(if state.battle_paused {
                "Resume"
            } else {
                "Pause"
            })
            .clicked()
        {
            state.battle_paused = !state.battle_paused;
        }
        ui.add(egui::Slider::new(&mut state.simulation_speed, 1..=4).text("Speed Multiplier"));
    });

    ui.separator();
    ui.heading("Player 0 (Blue) Micro Commands");
    ui.horizontal(|ui| {
        if ui.button("Attack-Move (High Ground)").clicked() {
            issue_sandbox_command(
                state,
                PlayerId(0),
                UnitOrder::AttackMove {
                    target: SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(32)),
                },
            );
        }
        if ui.button("Kite Retreat (Base)").clicked() {
            issue_sandbox_command(
                state,
                PlayerId(0),
                UnitOrder::Move {
                    target: SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(32)),
                },
            );
        }
    });

    ui.horizontal(|ui| {
        if ui.button("Hold Position").clicked() {
            issue_sandbox_command(state, PlayerId(0), UnitOrder::HoldPosition);
        }
        if ui.button("Stimpack (+Speed)").clicked() {
            issue_sandbox_command(state, PlayerId(0), UnitOrder::UseStim);
        }
    });

    ui.separator();
    if ui.button("Reset / Initialize Sandbox").clicked() {
        state.sim_world = Some(SimWorld::new_default_battle_sandbox());
    }
}

fn issue_sandbox_command(state: &mut LabState, player_id: PlayerId, order: UnitOrder) {
    if let Some(ref mut world) = state.sim_world {
        let entities: Vec<EntityId> = world
            .entities
            .values()
            .filter(|e| e.owner == player_id && !e.is_immobile)
            .map(|e| e.id)
            .collect();

        let cmd = PlayerCommand {
            player_id,
            entities,
            order,
            queue: false,
        };

        let bundle = TickBundle {
            tick: Tick(world.current_tick),
            commands: vec![cmd],
            state_checksum: None,
        };

        let _ = world.step(&bundle);
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
    ui.checkbox(
        &mut state.colorblind_mode,
        "Colorblind Palette (Cyan vs Orange)",
    );

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

                let active_name = arch.map_or("TACTICAL UNIT", |a| a.display_name.as_str());

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
                            // Holographic tactical radar fallback box
                            let (rect, _response) = ui.allocate_exact_size(
                                egui::vec2(200.0, 200.0),
                                egui::Sense::hover(),
                            );
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(12, 16, 28));
                            painter.rect_stroke(
                                rect,
                                4.0,
                                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 200, 255)),
                                egui::StrokeKind::Inside,
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("HOLO VIDEO FEED\n{active_name}"),
                                egui::FontId::monospace(14.0),
                                egui::Color32::from_rgb(0, 240, 255),
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

                        ui.horizontal(|ui| {
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
                        ui.horizontal(|ui| {
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
                            ui.horizontal(|ui| {
                                ui.label(format!("Weapon: {}", weapon.name));
                                ui.label(format!("Damage: {:.1}", weapon.base_damage.to_f32()));
                                ui.label(format!("Range: {:.1} u", weapon.max_range.to_f32()));
                                ui.label(format!(
                                    "Period: {:.2}s ({} ticks)",
                                    f32::from(weapon.period_ticks) / 25.0,
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

                        ui.horizontal(|ui| {
                            ui.label(format!("Mover: {:?}", arch.mover_type));
                            ui.label(format!("Life Armor: {:.0}", arch.life_armor.to_f32()));
                            ui.label(format!("Shield Armor: {:.0}", arch.shield_armor.to_f32()));
                            ui.label(format!("Aim Angle: {:.1}°", lab_state.aim_angle_deg));
                            ui.label(format!("Speed: {:.2} u/s", lab_state.speed));
                        });

                        ui.separator();

                        // Quick-Select Hero Showcase Lineup Buttons
                        ui.horizontal(|ui| {
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
