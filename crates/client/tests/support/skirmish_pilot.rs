//! A scripted player that plays the skirmish through the screen and the input devices alone.
//!
//! # Responsibility
//! Owns [`SkirmishPilotPlugin`], which plays one side of the shipped skirmish the way a person
//! would: it looks at what the client draws, and answers with keyboard and mouse events. It is the
//! exit proof of Phase 5 (decision D-13): "a person can play a match to a result using only mouse
//! and keyboard".
//!
//! # What it may read
//! Only what a person sees or knows before the match: the HUD's [`HudModel`] (which holds the
//! viewer-scoped `PlayerView`, so fog is already applied), [`BattleSelection`], [`CommandCard`],
//! [`PlacementPreview`], [`ResultScreen`], the rectangles of [`HudGeometry`] and [`UiGeometry`],
//! the map billboards of [`MapPresentation`], the camera, the map document (terrain, resource
//! nodes and start locations are public before the match), and bubbles whose
//! `InheritedVisibility` is true. It never reads the authoritative simulation: this file must not
//! name the session's world accessor, and `skirmish_match_tests.rs` fails if it does.
//!
//! # What it may write
//! Input events only: `CursorMoved`, `MouseButtonInput`, `MouseWheel` and `KeyboardInput`, at
//! most one per frame. With a cursor move it also records the cursor on the `Window`, as a window backend
//! does, because the headless client has no backend to do it.
//!
//! # How it fits
//! Its system runs in `Last`, after the frame has been drawn and every HUD rectangle recorded, so
//! the event it sends reaches the next frame exactly as a `Harness` gesture sent between two
//! updates does. A stage plans a short script of events from what is on screen, and the script is
//! played out one event per frame before the next plan.
//!
//! # Invariants
//! - No unit or structure identifier appears here. Which structure to raise and which unit to
//!   train come from the command card and the catalog's capability flags.
//! - The pilot is deterministic: under a fixed frame time its plans depend only on what the
//!   client shows, which depends only on earlier input.
//! - A stall is loud: past [`TICK_LIMIT`] the pilot panics with its stage and bank.

use std::collections::VecDeque;
use std::fmt::Write as _;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use client::battle_hud::card::{CardAction, CommandCard};
use client::battle_hud::catalog::ArchetypeCommands;
use client::battle_hud::model::HudModel;
use client::battle_hud::panels::{MAX_SIMULATION_SPEED, MIN_SIMULATION_SPEED};
use client::battle_hud::placement::PlacementPreview;
use client::battle_hud::result::{ResultScreen, ViewerResult};
use client::battle_hud::HudGeometry;
use client::battle_input::command_mode::CommandMode;
use client::battle_selection::BattleSelection;
use client::bubble_shader::BubbleInstance;
use client::input_bindings::{
    CONTROL_GROUP_KEYS, PAN_BACK_KEYS, PAN_FORWARD_KEYS, PAN_LEFT_KEYS, PAN_RIGHT_KEYS,
};
use client::input_policy::{world_to_window, UiGeometry};
use client::runtime_battle::{BattleDefinition, MapPresentation};
use client::scene_controller::LiveBattleBubble;
use client::RtsCamera;
use sim_map::ResourceKind;
use sim_types::{AnimState, CommandSubject, EntityId, PlayerId, UnitTypeId};

/// Simulation ticks after which a match that has not ended is a stall.
pub const TICK_LIMIT: u32 = 12_000;

/// Combat units the pilot trains before it marches.
pub const ARMY_SIZE: usize = 6;

/// Jobs the pilot keeps queued at its training structure at most.
const QUEUE_DEPTH: u32 = 2;

/// Control group of the builder, by index into [`CONTROL_GROUP_KEYS`].
const BUILDER_GROUP: usize = 1;

/// Control group of the army, by index into [`CONTROL_GROUP_KEYS`].
const ARMY_GROUP: usize = 0;

/// Distance from the start of the resource nodes the pilot mines, in world units. A base's own
/// nodes lie well inside it, and the other bases' far outside.
const HOME_NODE_RADIUS: f32 = 14.0;

/// Distances from the town hall the pilot tries sites at, in world units, nearest first.
const SITE_RADII: [f32; 4] = [6.0, 8.0, 10.0, 12.0];

/// Directions tried at each radius.
const SITE_DIRECTIONS: usize = 16;

/// Margin kept between a click and the edge of the world rectangle, in egui points.
const CLICK_MARGIN: f32 = 24.0;

/// Share of the world rectangle's half extent within which the march target counts as framed.
const FRAMED_SHARE: f32 = 0.4;

/// Frames a pan key is held per plan. At 25 world units per second and 40 ms frames this moves
/// the camera five units.
const PAN_PULSE_FRAMES: u32 = 5;

/// How far short of the enemy start the army is sent, in world units, so the click lands on
/// open ground rather than the hall.
const MARCH_SHORTFALL: f32 = 6.0;

/// Ticks without a visible enemy after which the pilot repeats its attack-move.
const REISSUE_TICKS: u32 = 250;

/// Wheel steps the pilot may take to fit its base on screen.
const MAX_ZOOM_STEPS: u32 = 20;

/// Padding of the selection box around the army, in egui points.
const BOX_PADDING: f32 = 24.0;

/// What the pilot is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PilotGoal {
    /// Play until the result window shows.
    MatchResult,
    /// Stop once a foundation of the chosen structure stands. Starts with no mining, so whoever
    /// runs it must fund the bank.
    FoundationRaised,
}

/// Plays the skirmish for the viewer of the HUD.
pub struct SkirmishPilotPlugin {
    /// Where to stop.
    pub goal: PilotGoal,
}

impl Plugin for SkirmishPilotPlugin {
    fn build(&self, app: &mut App) {
        install(app, self.goal);
    }
}

/// Adds the pilot to an app that has already finished building its plugins, as the headless
/// harness's has.
pub fn install(app: &mut App, goal: PilotGoal) {
    app.insert_resource(Pilot::new(goal)).add_systems(Last, fly);
}

/// The stages of a match, in the order they are played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for the HUD of a committed battle.
    Boot,
    /// Sending each worker to its own mineral, while the battle is paused.
    Mine,
    /// Starting the clock with the HUD's pause button.
    Resume,
    /// Raising the speed with the HUD's speed button.
    Speed,
    /// Waiting for the structure's price, then selecting the builder and opening its list.
    Raise,
    /// Moving the cursor around the base until the placement preview accepts the site.
    Site,
    /// Waiting for the foundation to appear.
    Foundation,
    /// Selecting the finished structure and training the army.
    Train,
    /// Boxing the army and assigning it a control group.
    Group,
    /// Panning the camera to the enemy start.
    March,
    /// Ordering the attack-move.
    Command,
    /// Clearing defenders, then attacking what remains.
    Assault,
    /// The goal is reached.
    Done,
}

/// One input event, or a pause between them.
#[derive(Debug, Clone, Copy)]
enum Step {
    Cursor(Vec2),
    Mouse(MouseButton, ButtonState),
    Key(KeyCode, ButtonState),
    /// Wheel lines; negative zooms out.
    Scroll(f32),
    Wait(u32),
}

/// The pilot's state, readable by the test that runs it.
#[derive(Resource)]
pub struct Pilot {
    goal: PilotGoal,
    stage: Stage,
    script: VecDeque<Step>,
    /// Workers not yet sent to mine, in ID order.
    to_mine: Vec<EntityId>,
    /// Mineral nodes not yet given a worker, nearest the start first.
    free_minerals: Vec<usize>,
    /// The mineral the builder returns to after placing.
    builder_mineral: Option<usize>,
    builder: Option<EntityId>,
    /// The structure chosen from the card, and the unit it trains.
    structure: Option<UnitTypeId>,
    soldier: Option<UnitTypeId>,
    /// Site candidates in map coordinates, tried in order.
    sites: VecDeque<Vec2>,
    /// The candidate the cursor was last moved to.
    trying: Option<Vec2>,
    /// The enemy dome the army was last sent against.
    target: Option<EntityId>,
    /// Wheel steps taken to fit the base on screen.
    zoom_steps: u32,
    /// Clicks on the speed button so far.
    speed_clicks: u32,
    /// Tick of the last attack-move.
    last_order_tick: u32,
    /// Where the army attack-moves to, in map coordinates.
    march_target: Option<Vec2>,
    /// The result the window showed.
    result: Option<ViewerResult>,
    /// Every stage entered, with the tick, for the record.
    pub log: String,
}

impl Pilot {
    fn new(goal: PilotGoal) -> Self {
        Self {
            goal,
            stage: Stage::Boot,
            script: VecDeque::new(),
            to_mine: Vec::new(),
            free_minerals: Vec::new(),
            builder_mineral: None,
            builder: None,
            structure: None,
            soldier: None,
            sites: VecDeque::new(),
            trying: None,
            target: None,
            zoom_steps: 0,
            speed_clicks: 0,
            last_order_tick: 0,
            march_target: None,
            result: None,
            log: String::new(),
        }
    }

    /// The stage being played.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// The result the window showed, once the goal is a match result and it has shown.
    #[must_use]
    pub const fn result(&self) -> Option<ViewerResult> {
        self.result
    }

    fn enter(&mut self, stage: Stage, tick: u32) {
        self.stage = stage;
        let _ = writeln!(self.log, "tick {tick}: {stage:?}");
        // On stderr as well, so a run killed by a deadline still shows how far it got.
        eprintln!("pilot: tick {tick}: {stage:?}");
    }

    fn tap(&mut self, key: KeyCode) {
        self.script.push_back(Step::Key(key, ButtonState::Pressed));
        self.script.push_back(Step::Key(key, ButtonState::Released));
    }

    fn click(&mut self, button: MouseButton, at: Vec2) {
        self.script.push_back(Step::Cursor(at));
        self.script
            .push_back(Step::Mouse(button, ButtonState::Pressed));
        self.script
            .push_back(Step::Mouse(button, ButtonState::Released));
    }

    fn with_held(&mut self, modifier: KeyCode, steps: impl FnOnce(&mut Self)) {
        self.script
            .push_back(Step::Key(modifier, ButtonState::Pressed));
        steps(self);
        self.script
            .push_back(Step::Key(modifier, ButtonState::Released));
    }
}

/// A visible bubble, as the pilot sees it.
#[derive(Clone, Copy)]
struct Seen {
    id: EntityId,
    owner: PlayerId,
    unit_type: UnitTypeId,
    anim: AnimState,
    /// Bevy world position of its centre.
    at: Vec3,
    /// Height of the ground under it, in world units.
    ground: f32,
}

type VisibleBubbles<'w, 's> = Query<
    'w,
    's,
    (
        &'static BubbleInstance,
        &'static GlobalTransform,
        &'static InheritedVisibility,
    ),
    With<LiveBattleBubble>,
>;

/// Everything the pilot looks at on one frame.
#[derive(bevy::ecs::system::SystemParam)]
struct Screen<'w, 's> {
    model: Res<'w, HudModel>,
    selection: Res<'w, BattleSelection>,
    card: Res<'w, CommandCard>,
    mode: Res<'w, CommandMode>,
    preview: Res<'w, PlacementPreview>,
    result: Res<'w, ResultScreen>,
    hud: Res<'w, HudGeometry>,
    ui: Res<'w, UiGeometry>,
    map: Option<Res<'w, MapPresentation>>,
    definition: Res<'w, BattleDefinition>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<RtsCamera>>,
    bubbles: VisibleBubbles<'w, 's>,
}

impl Screen<'_, '_> {
    fn tick(&self) -> u32 {
        self.model.view.tick.0
    }

    fn viewer(&self) -> Option<PlayerId> {
        self.model.viewer
    }

    fn seen(&self) -> impl Iterator<Item = Seen> + '_ {
        self.bubbles
            .iter()
            .filter(|(_, _, visible)| visible.get())
            .map(|(bubble, transform, _)| Seen {
                id: bubble.0.entity_id,
                owner: bubble.0.owner,
                unit_type: bubble.0.unit_type,
                anim: bubble.0.anim_state,
                at: transform.translation(),
                ground: bubble.0.ground_z,
            })
    }

    fn own(&self) -> Vec<Seen> {
        let viewer = self.viewer();
        let mut own: Vec<Seen> = self.seen().filter(|s| Some(s.owner) == viewer).collect();
        own.sort_by_key(|s| s.id);
        own
    }

    fn is(&self, unit_type: UnitTypeId, wanted: impl Fn(&ArchetypeCommands) -> bool) -> bool {
        self.model.catalog.archetype(unit_type).is_some_and(wanted)
    }

    fn workers(&self) -> Vec<Seen> {
        self.own()
            .into_iter()
            .filter(|s| self.is(s.unit_type, |a| a.mobile && a.harvester))
            .collect()
    }

    /// The viewer's first structure: its town hall, at the start.
    fn hall(&self) -> Option<Seen> {
        self.own()
            .into_iter()
            .find(|s| self.is(s.unit_type, |a| a.structure))
    }

    /// Window position of world point `point`, if it is inside the world rectangle with margin.
    fn on_screen(&self, point: Vec3) -> Option<Vec2> {
        let (camera, transform) = self.cameras.get_single().ok()?;
        let at = world_to_window(camera, transform, point)?;
        self.ui
            .world?
            .shrink(CLICK_MARGIN)
            .contains(egui::pos2(at.x, at.y))
            .then_some(at)
    }

    /// Bevy world position of map point `point` at height `ground`.
    fn ground(point: Vec2, ground: f32) -> Vec3 {
        Vec3::new(point.x, ground, point.y)
    }

    /// Centre of `player`'s start location, in map coordinates.
    #[expect(
        clippy::cast_precision_loss,
        reason = "map cells are far below f32's exact integer range"
    )]
    fn start_of(&self, player: PlayerId) -> Option<Vec2> {
        self.definition
            .document
            .start_locations
            .iter()
            .find(|start| PlayerId(start.player_slot) == player)
            .map(|start| {
                Vec2::new(
                    start.cell.0 as f32 + f32::from(start.footprint.0) * 0.5,
                    start.cell.1 as f32 + f32::from(start.footprint.1) * 0.5,
                )
            })
    }

    fn enemy_start(&self) -> Option<Vec2> {
        let viewer = self.viewer()?;
        self.definition
            .document
            .start_locations
            .iter()
            .map(|start| PlayerId(start.player_slot))
            .find(|&player| player != viewer)
            .and_then(|player| self.start_of(player))
    }

    /// Billboard centre of node `index`, if it is drawn.
    fn node(&self, index: usize) -> Option<Vec3> {
        let object = self.map.as_ref()?.objects.get(index)?;
        object.visible.then_some(object.position)
    }

    /// The card slot whose action `wanted` accepts, with its key.
    fn slot(&self, wanted: impl Fn(&CardAction) -> bool) -> Option<(KeyCode, bool)> {
        self.card
            .slots
            .iter()
            .flatten()
            .find(|slot| wanted(&slot.action))
            .map(|slot| (slot.key, slot.enabled))
    }

    /// Price of `subject` from the view, if the viewer can make it.
    fn price(&self, subject: CommandSubject) -> Option<(u32, u32, u32)> {
        self.model
            .view
            .options
            .iter()
            .find(|option| option.subject == subject && option.lock.is_none())
            .map(|option| (option.minerals, option.gas, option.supply))
    }

    fn affords(&self, subject: CommandSubject) -> bool {
        let status = &self.model.view.status;
        self.price(subject).is_some_and(|(minerals, gas, supply)| {
            status.minerals >= minerals
                && status.gas >= gas
                && status.supply_used + supply <= status.supply_max
        })
    }

    fn bank(&self) -> String {
        let status = &self.model.view.status;
        format!(
            "{} minerals, {} gas, supply {}/{}",
            status.minerals, status.gas, status.supply_used, status.supply_max
        )
    }
}

/// Sends the next scripted event, or plans the next one.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
fn fly(
    mut pilot: ResMut<'_, Pilot>,
    screen: Screen<'_, '_>,
    mut windows: Query<'_, '_, (Entity, &mut Window), With<PrimaryWindow>>,
    mut cursor: EventWriter<'_, CursorMoved>,
    mut buttons: EventWriter<'_, MouseButtonInput>,
    mut keys: EventWriter<'_, KeyboardInput>,
    mut wheel: EventWriter<'_, MouseWheel>,
) {
    let tick = screen.tick();
    assert!(
        tick <= TICK_LIMIT,
        "the pilot stalled in {:?} at tick {tick} with {}\n{}",
        pilot.stage,
        screen.bank(),
        pilot.log
    );
    let Ok((window, mut native)) = windows.get_single_mut() else {
        return;
    };
    if pilot.script.is_empty() {
        plan(&mut pilot, &screen);
    }
    match pilot.script.pop_front() {
        Some(Step::Cursor(position)) => {
            native.set_cursor_position(Some(position));
            cursor.send(CursorMoved {
                window,
                position,
                delta: None,
            });
        }
        Some(Step::Mouse(button, state)) => {
            buttons.send(MouseButtonInput {
                button,
                state,
                window,
            });
        }
        Some(Step::Key(key_code, state)) => {
            keys.send(KeyboardInput {
                key_code,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state,
                repeat: false,
                window,
            });
        }
        Some(Step::Scroll(lines)) => {
            wheel.send(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: lines,
                window,
            });
        }
        Some(Step::Wait(frames)) if frames > 1 => {
            pilot.script.push_front(Step::Wait(frames - 1));
        }
        Some(Step::Wait(_)) | None => {}
    }
}

/// Decides what to do next from what is on screen, and queues the events for it.
#[expect(
    clippy::too_many_lines,
    reason = "one player's match, read top to bottom"
)]
fn plan(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    match pilot.stage {
        Stage::Boot => {
            if screen.viewer().is_none() || screen.hud.pause.is_none() || screen.map.is_none() {
                return;
            }
            let Some(hall) = screen.hall() else {
                return;
            };
            let start = Vec2::new(hall.at.x, hall.at.z);
            let mut minerals: Vec<(usize, f32)> = screen
                .definition
                .document
                .resources
                .iter()
                .enumerate()
                .filter(|(_, node)| node.kind == ResourceKind::Mineral)
                .filter_map(|(index, _)| {
                    let at = screen.node(index)?;
                    let distance = Vec2::new(at.x, at.z).distance(start);
                    (distance < HOME_NODE_RADIUS).then_some((index, distance))
                })
                .collect();
            minerals.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            let workers = screen.workers();
            // Zoom out until every worker and the minerals they will mine are in view.
            let in_view = workers.iter().all(|w| screen.on_screen(w.at).is_some())
                && minerals.iter().take(workers.len()).all(|(index, _)| {
                    screen
                        .node(*index)
                        .and_then(|at| screen.on_screen(at))
                        .is_some()
                });
            if !in_view {
                assert!(
                    pilot.zoom_steps < MAX_ZOOM_STEPS,
                    "the base does not fit on screen\n{}",
                    pilot.log
                );
                pilot.zoom_steps += 1;
                let world = screen.ui.world.expect("a battle is on screen");
                pilot.script.push_back(Step::Cursor(centre(world)));
                pilot.script.push_back(Step::Scroll(-1.0));
                pilot.script.push_back(Step::Wait(2));
                return;
            }
            pilot.free_minerals = minerals.into_iter().map(|(index, _)| index).collect();
            pilot.to_mine = workers.iter().map(|w| w.id).collect();
            pilot.builder = pilot.to_mine.first().copied();
            let next = match pilot.goal {
                PilotGoal::MatchResult => Stage::Mine,
                PilotGoal::FoundationRaised => Stage::Raise,
            };
            pilot.enter(next, tick);
        }
        Stage::Mine => {
            if pilot.to_mine.is_empty() {
                pilot.enter(Stage::Resume, tick);
                return;
            }
            let worker = pilot.to_mine.remove(0);
            let seen = screen.own().into_iter().find(|s| s.id == worker);
            let (Some(seen), Some(&node)) = (seen, pilot.free_minerals.first()) else {
                panic!(
                    "worker {worker:?} or a free mineral is off screen\n{}",
                    pilot.log
                );
            };
            let worker_at = screen
                .on_screen(seen.at)
                .unwrap_or_else(|| panic!("worker {worker:?} is outside the world view"));
            let node_at = screen
                .node(node)
                .and_then(|at| screen.on_screen(at))
                .unwrap_or_else(|| panic!("mineral {node} is outside the world view"));
            pilot.free_minerals.remove(0);
            pilot.click(MouseButton::Left, worker_at);
            if Some(worker) == pilot.builder {
                pilot.builder_mineral = Some(node);
                pilot.with_held(KeyCode::ControlLeft, |pilot| {
                    pilot.tap(CONTROL_GROUP_KEYS[BUILDER_GROUP]);
                });
            }
            pilot.click(MouseButton::Right, node_at);
        }
        Stage::Resume => {
            let pause = screen.hud.pause.expect("the resource bar is up");
            pilot.click(MouseButton::Left, centre(pause));
            pilot.enter(
                match pilot.goal {
                    PilotGoal::MatchResult => Stage::Speed,
                    PilotGoal::FoundationRaised => Stage::Foundation,
                },
                tick,
            );
        }
        Stage::Speed => {
            // The bar starts at its slowest speed, and each click raises it one step.
            if pilot.speed_clicks == MAX_SIMULATION_SPEED - MIN_SIMULATION_SPEED {
                pilot.enter(Stage::Raise, tick);
                return;
            }
            let button = screen.hud.speed_up.expect("the resource bar is up");
            pilot.speed_clicks += 1;
            pilot.click(MouseButton::Left, centre(button));
        }
        Stage::Raise => plan_raise(pilot, screen),
        Stage::Site => plan_site(pilot, screen),
        Stage::Foundation => {
            let structure = pilot
                .structure
                .expect("invariant: a site is placed only after a structure is chosen");
            let raised = screen
                .own()
                .iter()
                .any(|s| s.unit_type == structure && s.anim == AnimState::Constructing);
            if raised {
                let next = match pilot.goal {
                    PilotGoal::MatchResult => Stage::Train,
                    PilotGoal::FoundationRaised => Stage::Done,
                };
                pilot.enter(next, tick);
            }
        }
        Stage::Train => plan_train(pilot, screen),
        Stage::Group => plan_group(pilot, screen),
        Stage::March => plan_march(pilot, screen),
        Stage::Command => {
            let group = CONTROL_GROUP_KEYS[ARMY_GROUP];
            if *screen.mode == CommandMode::AttackMove {
                let target = pilot.march_target.expect("set when the march began");
                let ground = screen.hall().map_or(0.0, |hall| hall.ground);
                let at = screen
                    .on_screen(Screen::ground(target, ground))
                    .expect("the march framed its target");
                pilot.click(MouseButton::Left, at);
                pilot.last_order_tick = tick;
                pilot.enter(Stage::Assault, tick);
                return;
            }
            assert!(
                !screen.selection.group(ARMY_GROUP).is_empty(),
                "the army was lost at tick {tick}\n{}",
                pilot.log
            );
            if screen.selection.ids() != screen.selection.group(ARMY_GROUP) {
                pilot.tap(group);
                return;
            }
            match screen.slot(|action| *action == CardAction::Mode(CommandMode::AttackMove)) {
                Some((key, true)) => pilot.tap(key),
                _ => pilot.script.push_back(Step::Wait(1)),
            }
        }
        Stage::Assault => plan_assault(pilot, screen),
        Stage::Done => {}
    }
}

/// Raise: wait for the price, select the builder, open its list and pick the structure.
fn plan_raise(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    if let Some(structure) = pilot.structure {
        if !screen.affords(CommandSubject::Raise(structure)) {
            return;
        }
    }
    let builder = pilot.builder.expect("chosen at boot");
    if screen.selection.ids() != [builder] {
        if pilot.goal == PilotGoal::MatchResult {
            pilot.tap(CONTROL_GROUP_KEYS[BUILDER_GROUP]);
        } else {
            let seen = screen.own().into_iter().find(|s| s.id == builder);
            let at = seen
                .and_then(|s| screen.on_screen(s.at))
                .expect("the builder is in view");
            pilot.click(MouseButton::Left, at);
        }
        return;
    }
    if let CommandMode::Place(structure) = *screen.mode {
        pilot.structure = Some(structure);
        let hall = screen.hall().expect("the town hall stands");
        let start = Vec2::new(hall.at.x, hall.at.z);
        // Sites facing away from the minerals first, so the building cannot block the mining.
        let minerals = pilot
            .builder_mineral
            .and_then(|node| screen.node(node))
            .map_or(start, |at| Vec2::new(at.x, at.z));
        let away = (start - minerals).normalize_or(Vec2::X);
        let mut sites: Vec<(Vec2, f32, usize)> = Vec::new();
        for (ring, radius) in SITE_RADII.iter().enumerate() {
            for step in 0..SITE_DIRECTIONS {
                #[expect(clippy::cast_precision_loss, reason = "a handful of directions")]
                let direction =
                    Vec2::from_angle(step as f32 * std::f32::consts::TAU / SITE_DIRECTIONS as f32);
                sites.push((start + direction * *radius, -direction.dot(away), ring));
            }
        }
        sites.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.total_cmp(&b.1)));
        pilot.sites = sites.into_iter().map(|(site, _, _)| site).collect();
        pilot.enter(Stage::Site, tick);
        return;
    }
    if let Some(structure) = pilot.structure {
        // Back on the card after a refused site: choose the same structure again.
        if let Some((key, true)) =
            screen.slot(|action| *action == CardAction::Mode(CommandMode::Place(structure)))
        {
            pilot.tap(key);
            return;
        }
    }
    let raise = screen
        .card
        .slots
        .iter()
        .flatten()
        .find_map(|slot| match slot.action {
            CardAction::Mode(CommandMode::Place(structure))
                if trains_an_army(screen, structure) =>
            {
                Some((structure, slot.key, slot.enabled))
            }
            _ => None,
        });
    match raise {
        Some((structure, key, true)) => {
            if !screen.affords(CommandSubject::Raise(structure)) {
                pilot.structure = Some(structure);
                return;
            }
            pilot.tap(key);
        }
        Some((_, _, false)) => panic!("the structure that trains an army is locked\n{}", pilot.log),
        None => match screen.slot(|action| *action == CardAction::OpenBuildMenu) {
            Some((key, true)) => pilot.tap(key),
            _ => pilot.script.push_back(Step::Wait(1)),
        },
    }
}

/// Whether `structure` trains a mobile armed unit that does not gather: a combat unit.
fn trains_an_army(screen: &Screen<'_, '_>, structure: UnitTypeId) -> bool {
    screen
        .model
        .catalog
        .archetype(structure)
        .is_some_and(|archetype| {
            archetype
                .trains
                .iter()
                .any(|unit| screen.is(*unit, |a| a.mobile && a.armed && !a.harvester))
        })
}

/// Site: try candidates until the preview accepts one, then place it and send the builder back.
fn plan_site(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    if !matches!(*screen.mode, CommandMode::Place(_)) {
        // Placed: the mode ended with the click.
        let node = pilot.builder_mineral;
        if let Some(at) = node
            .and_then(|node| screen.node(node))
            .and_then(|at| screen.on_screen(at))
        {
            pilot.with_held(KeyCode::ShiftLeft, |pilot| {
                pilot.click(MouseButton::Right, at);
            });
        }
        pilot.enter(
            match pilot.goal {
                PilotGoal::MatchResult => Stage::Foundation,
                PilotGoal::FoundationRaised => Stage::Resume,
            },
            tick,
        );
        return;
    }
    if pilot.trying.is_some()
        && screen.preview.cell.is_some()
        && screen.preview.verdict == Some(Ok(()))
    {
        pilot
            .script
            .push_back(Step::Mouse(MouseButton::Left, ButtonState::Pressed));
        pilot
            .script
            .push_back(Step::Mouse(MouseButton::Left, ButtonState::Released));
        pilot.trying = None;
        return;
    }
    let hall = screen.hall().expect("the town hall stands");
    while let Some(site) = pilot.sites.pop_front() {
        if let Some(at) = screen.on_screen(Screen::ground(site, hall.ground)) {
            pilot.trying = Some(site);
            pilot.script.push_back(Step::Cursor(at));
            // The preview answers in the frame that reads the cursor.
            pilot.script.push_back(Step::Wait(1));
            return;
        }
    }
    panic!("no site around the base was accepted\n{}", pilot.log);
}

/// Train: select the finished structure and keep its queue short until the army is complete.
fn plan_train(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    let structure = pilot.structure.expect("chosen before the site");
    let own = screen.own();
    let Some(building) = own
        .iter()
        .find(|s| s.unit_type == structure && s.anim != AnimState::Constructing)
    else {
        return;
    };
    if screen.selection.ids() != [building.id] {
        let at = screen
            .on_screen(building.at)
            .expect("the structure stands in view of the base");
        pilot.click(MouseButton::Left, at);
        return;
    }
    if pilot.soldier.is_none() {
        let combat = |unit: UnitTypeId| screen.is(unit, |a| a.mobile && a.armed && !a.harvester);
        let Some(soldier) = screen.card.slots.iter().flatten().find_map(|slot| {
            if let CardAction::Train(unit) = slot.action {
                (slot.enabled && combat(unit)).then_some(unit)
            } else {
                None
            }
        }) else {
            pilot.script.push_back(Step::Wait(1));
            return;
        };
        pilot.soldier = Some(soldier);
    }
    let soldier = pilot.soldier.expect("invariant: chosen just above");
    let army = own.iter().filter(|s| s.unit_type == soldier).count();
    if army >= ARMY_SIZE {
        pilot.enter(Stage::Group, tick);
        return;
    }
    let queued = screen
        .model
        .view
        .queues
        .iter()
        .find(|queue| queue.entity == building.id)
        .map_or(0, |queue| queue.job_count);
    let wanted = u32::try_from(ARMY_SIZE - army).unwrap_or(u32::MAX);
    if queued < QUEUE_DEPTH.min(wanted) && screen.affords(CommandSubject::Train(soldier)) {
        if let Some((key, true)) = screen.slot(|action| *action == CardAction::Train(soldier)) {
            pilot.tap(key);
        }
    }
}

/// Group: box the army, check the box took it, and assign it a control group.
fn plan_group(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    let soldier = pilot.soldier.expect("chosen while training");
    let army: Vec<Seen> = screen
        .own()
        .into_iter()
        .filter(|s| s.unit_type == soldier)
        .collect();
    let all_selected = army.iter().all(|s| screen.selection.contains(s.id));
    if all_selected && !army.is_empty() {
        pilot.with_held(KeyCode::ControlLeft, |pilot| {
            pilot.tap(CONTROL_GROUP_KEYS[ARMY_GROUP]);
        });
        pilot.march_target = screen.enemy_start().map(|enemy| {
            let home = screen
                .start_of(screen.viewer().expect("set at boot"))
                .unwrap_or(enemy);
            enemy + (home - enemy).normalize_or(Vec2::ZERO) * MARCH_SHORTFALL
        });
        pilot.enter(Stage::March, tick);
        return;
    }
    let points: Vec<Vec2> = army
        .iter()
        .map(|s| screen.on_screen(s.at).expect("the new army stands in view"))
        .collect();
    let (low, high) = points.iter().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    );
    let world = screen.ui.world.expect("a battle is on screen");
    let clamp = |p: Vec2| {
        Vec2::new(
            p.x.clamp(world.left() + 1.0, world.right() - 1.0),
            p.y.clamp(world.top() + 1.0, world.bottom() - 1.0),
        )
    };
    let from = clamp(low - Vec2::splat(BOX_PADDING));
    let to = clamp(high + Vec2::splat(BOX_PADDING));
    pilot.script.push_back(Step::Cursor(from));
    pilot
        .script
        .push_back(Step::Mouse(MouseButton::Left, ButtonState::Pressed));
    pilot.script.push_back(Step::Cursor((from + to) * 0.5));
    pilot.script.push_back(Step::Cursor(to));
    pilot
        .script
        .push_back(Step::Mouse(MouseButton::Left, ButtonState::Released));
}

/// March: pan by held keys until the target is framed.
fn plan_march(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    let target = pilot.march_target.expect("set when grouping");
    let ground = screen.hall().map_or(0.0, |hall| hall.ground);
    let Ok((camera, transform)) = screen.cameras.get_single() else {
        return;
    };
    let world = screen.ui.world.expect("a battle is on screen");
    let centre = Vec2::new(world.center().x, world.center().y);
    let half = Vec2::new(world.width(), world.height()) * 0.5 * FRAMED_SHARE;
    let projected = world_to_window(camera, transform, Screen::ground(target, ground));
    let offset = projected.map(|at| at - centre);
    if offset.is_some_and(|offset| offset.x.abs() <= half.x && offset.y.abs() <= half.y) {
        pilot.enter(Stage::Command, tick);
        return;
    }
    // Screen right is map +x and screen up is map -y under this camera, so the keys follow the
    // offset on screen; a point behind the camera is beyond the top of the screen.
    let offset = offset.unwrap_or(Vec2::new(0.0, -world.height()));
    let mut held = Vec::new();
    if offset.x > half.x {
        held.push(PAN_RIGHT_KEYS[0]);
    } else if offset.x < -half.x {
        held.push(PAN_LEFT_KEYS[0]);
    }
    if offset.y > half.y {
        held.push(PAN_BACK_KEYS[0]);
    } else if offset.y < -half.y {
        held.push(PAN_FORWARD_KEYS[0]);
    }
    for key in &held {
        pilot
            .script
            .push_back(Step::Key(*key, ButtonState::Pressed));
    }
    pilot.script.push_back(Step::Wait(PAN_PULSE_FRAMES));
    for key in &held {
        pilot
            .script
            .push_back(Step::Key(*key, ButtonState::Released));
    }
}

/// Assault: attack-move until no defender is in sight, then attack the remaining domes.
fn plan_assault(pilot: &mut Pilot, screen: &Screen<'_, '_>) {
    let tick = screen.tick();
    if let Some(verdict) = screen.result.verdict() {
        pilot.result = Some(verdict);
        pilot.enter(Stage::Done, tick);
        return;
    }
    let viewer = screen.viewer().expect("set at boot");
    let enemies: Vec<Seen> = screen.seen().filter(|s| s.owner != viewer).collect();
    let defended = enemies.iter().any(|s| screen.is(s.unit_type, |a| a.mobile));
    if defended {
        pilot.last_order_tick = tick;
        if pilot.target.take().is_some() {
            // A focused army does not answer defenders, so go back to attack-moving.
            pilot.enter(Stage::Command, tick);
        }
        return;
    }
    let dome = enemies
        .iter()
        .filter(|s| screen.is(s.unit_type, |a| a.structure))
        .min_by_key(|s| s.id);
    if screen.selection.ids() != screen.selection.group(ARMY_GROUP) {
        pilot.tap(CONTROL_GROUP_KEYS[ARMY_GROUP]);
        return;
    }
    match dome {
        Some(dome) if pilot.target != Some(dome.id) => {
            if let Some(at) = screen.on_screen(dome.at) {
                pilot.target = Some(dome.id);
                pilot.last_order_tick = tick;
                pilot.click(MouseButton::Right, at);
            }
        }
        None if tick.saturating_sub(pilot.last_order_tick) > REISSUE_TICKS => {
            pilot.target = None;
            pilot.enter(Stage::Command, tick);
        }
        Some(_) | None => {}
    }
}

fn centre(rect: egui::Rect) -> Vec2 {
    Vec2::new(rect.center().x, rect.center().y)
}
