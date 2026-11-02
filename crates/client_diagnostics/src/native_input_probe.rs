//! Passive evidence for real native keyboard, pointer and camera interaction.
//!
//! The probe uses the production application and renderer. Its only fixture writes
//! pause the initial battle and choose window dimensions; subsequent interaction
//! must arrive through the native window. A reusable buffered CSV writer records
//! events and final frame state without controlling camera, selection or UI.

mod button_delivery;
mod pipeline;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use clap::Parser;

/// Native evidence options; window dimensions are physical pixels at scale one.
#[derive(Parser)]
pub struct ProbeOptions {
    /// New CSV evidence file; existing evidence is never overwritten.
    #[arg(long)]
    pub trace: PathBuf,
    /// Initial logical window width.
    #[arg(long, default_value_t = 1280)]
    pub width: u32,
    /// Initial logical window height.
    #[arg(long, default_value_t = 720)]
    pub height: u32,
    /// Explicit window scale for reproducible non-unit-DPI verification.
    #[arg(long, default_value_t = 1.0)]
    pub scale: f32,
    /// Optional new CSV for passive authoritative/presented/actual-component evidence.
    #[arg(long)]
    pub pipeline_trace: Option<PathBuf>,
    /// Explicit virtual frame duration for the pipeline probe; not measured display FPS.
    #[arg(long, requires = "pipeline_trace")]
    pub frame_step_nanos: Option<u64>,
}

#[derive(Resource)]
struct Evidence {
    writer: BufWriter<File>,
    frame: u64,
    keys: u64,
    wheels: u64,
    motions: u64,
    cursors: u64,
    failed: Arc<AtomicBool>,
    key_delivery: KeyDelivery,
    button_delivery: button_delivery::ButtonDelivery,
}

#[derive(Default)]
struct KeyDelivery {
    last_code: Option<KeyCode>,
    last_pressed: bool,
    control_down: u64,
    control_up: u64,
    f1_down: u64,
    f1_up: u64,
}

impl KeyDelivery {
    fn record(&mut self, event: &KeyboardInput) {
        self.last_code = Some(event.key_code);
        self.last_pressed = event.state.is_pressed();
        let counters = match event.key_code {
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                Some((&mut self.control_down, &mut self.control_up))
            }
            KeyCode::F1 => Some((&mut self.f1_down, &mut self.f1_up)),
            _ => None,
        };
        if let Some((down, up)) = counters {
            if self.last_pressed {
                *down += 1;
            } else {
                *up += 1;
            }
        }
    }

    fn write(
        &self,
        writer: &mut impl Write,
        keyboard: &ButtonInput<KeyCode>,
    ) -> std::io::Result<()> {
        write!(
            writer,
            ",{},{},{:?},{},{},{},{},{}",
            u8::from(
                keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)
            ),
            u8::from(keyboard.pressed(KeyCode::F1)),
            self.last_code,
            u8::from(self.last_pressed),
            self.control_down,
            self.control_up,
            self.f1_down,
            self.f1_up
        )
    }
}

/// Starts the real client with passive event/frame recording.
///
/// # Errors
/// Returns typed startup, option validation or evidence-file failures.
#[expect(
    clippy::cast_precision_loss,
    reason = "capture coordinates are bounded by the window size and frame budget"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn run(options: ProbeOptions) -> Result<AppExit, ProbeError> {
    if options.width == 0
        || options.height == 0
        || !options.scale.is_finite()
        || options.scale <= 0.0
        || options
            .frame_step_nanos
            .is_some_and(|step| step == 0 || step > 1_000_000_000)
    {
        return Err(ProbeError::Dimensions);
    }
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(&options.trace)?;
    let mut writer = BufWriter::with_capacity(64 * 1024, file);
    write!(writer, "frame,keys,wheels,motions,focused,keyboard_ui,pointer_ui,target_x,target_z,distance,selected,tab,tick,paused,width,height,scale,egui_scale,cursor_x,cursor_y,subject_id,subject_x,subject_y,w,a,s,d,middle")?;
    for name in [
        "lab", "battle", "swarm", "health", "pause", "reset", "world",
    ] {
        write!(writer, ",{name}_x,{name}_y,{name}_w,{name}_h")?;
    }
    write!(writer, ",cursor_events,inspector_unit,subject_unit,ctrl,f1,last_key,last_key_pressed,ctrl_down,ctrl_up,f1_down,f1_up")?;
    writeln!(writer, "{}", button_delivery::HEADER)?;
    writer.flush()?;
    let mut app = client::build_graphical_app(
        client::ClientMode::VisualLab,
        None,
        std::path::Path::new(client::runtime_battle::DEFAULT_BATTLE_SCENARIO),
    )?;
    let mut windows = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    let mut window = windows
        .get_single_mut(app.world_mut())
        .map_err(|_| ProbeError::Window)?;
    window.title = "Space Scuffle — native input verification".into();
    window
        .resolution
        .set_scale_factor_override(Some(options.scale));
    window
        .resolution
        .set(options.width as f32, options.height as f32);
    app.world_mut()
        .resource_mut::<client::LabState>()
        .battle_paused = true;
    let failed = Arc::new(AtomicBool::new(false));
    let media_policy = app
        .world()
        .resource::<client::media::MediaFailurePolicy>()
        .clone();
    if let Some(path) = &options.pipeline_trace {
        pipeline::install(&mut app, path, failed.clone())?;
    }
    if let Some(nanos) = options.frame_step_nanos {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_nanos(nanos),
        ));
    }
    app.insert_resource(Evidence {
        writer,
        frame: 0,
        keys: 0,
        wheels: 0,
        motions: 0,
        cursors: 0,
        failed: failed.clone(),
        key_delivery: KeyDelivery::default(),
        button_delivery: button_delivery::ButtonDelivery::default(),
    })
    .add_systems(Last, (button_delivery::record, observe).chain());
    let exit = media_policy.final_exit(client::render_compatibility::run_render_app(&mut app));
    if failed.load(Ordering::Acquire) && matches!(exit, AppExit::Success) {
        return Err(ProbeError::Recording);
    }
    Ok(exit)
}

/// Explicit native probe startup and recording failures.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Window dimensions/scale must be finite and positive.
    #[error(
        "native probe requires positive dimensions/scale and a virtual step in 1..=1000000000ns"
    )]
    Dimensions,
    /// The shared production builder must create one primary window.
    #[error("native probe requires exactly one production primary window")]
    Window,
    /// The real production client could not start.
    #[error(transparent)]
    Startup(#[from] client::ClientStartupError),
    /// Evidence must be created and written successfully.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A recording error already reported through the production exit path.
    #[error("native input evidence recording failed")]
    Recording,
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn observe(
    mut evidence: ResMut<'_, Evidence>,
    mut keys: EventReader<'_, '_, KeyboardInput>,
    mut wheels: EventReader<'_, '_, MouseWheel>,
    mut motions: EventReader<'_, '_, MouseMotion>,
    mut cursors: EventReader<'_, '_, CursorMoved>,
    geometry: Res<'_, client::input_policy::UiGeometry>,
    catalog: Res<'_, client::CatalogResource>,
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    mouse: Res<'_, ButtonInput<MouseButton>>,
    windows: Query<'_, '_, &Window, With<PrimaryWindow>>,
    cameras: Query<'_, '_, (&client::RtsCamera, &Camera, &GlobalTransform)>,
    subjects: Query<
        '_,
        '_,
        (
            &client::BubbleInstance,
            &GlobalTransform,
            &client::HeroLineupIndex,
        ),
    >,
    mut contexts: EguiContexts<'_, '_>,
    selection: Res<'_, client::SelectionState>,
    battle: (
        Res<'_, client::LabState>,
        Res<'_, client::battle_pipeline::session::BattleSession>,
    ),
    mut exit: EventWriter<'_, AppExit>,
) {
    let (lab, session) = battle;
    for event in keys.read() {
        evidence.keys += 1;
        evidence.key_delivery.record(event);
    }
    evidence.wheels += wheels.read().count() as u64;
    evidence.motions += motions.read().count() as u64;
    evidence.cursors += cursors.read().count() as u64;
    let (Ok(window), Ok((camera, lens, transform)), Some(ctx)) = (
        windows.get_single(),
        cameras.get_single(),
        contexts.try_ctx_mut(),
    ) else {
        return;
    };
    let mut subject = (u32::MAX, Vec2::splat(-1.0));
    let mut subject_unit = u16::MAX;
    for (instance, position, index) in &subjects {
        if index.0 == lab.selected_hero_index {
            if let Some(projected) =
                client::input_policy::world_to_window(lens, transform, position.translation())
            {
                subject = (instance.0.entity_id.0, projected);
                subject_unit = instance.0.unit_type.0;
            }
        }
    }
    let cursor = window.cursor_position().unwrap_or(Vec2::splat(-1.0));
    let tab = match lab.current_tab {
        client::LabTab::VisualLab => 0,
        client::LabTab::LiveBattle => 1,
        client::LabTab::SwarmStressTest => 2,
    };
    let Evidence {
        writer,
        frame,
        keys,
        wheels,
        motions,
        cursors,
        failed,
        key_delivery,
        button_delivery,
    } = &mut *evidence;
    *frame += 1;
    // PERFORMANCE RATIONALE:
    // Why: Evidence must not allocate a formatted String on each observed frame.
    // What: Write numeric fields directly into one persistent buffered stream.
    // Trade-off: Probe-only file I/O is explicit; this is not a performance harness.
    let result = write!(writer, "{frame},{keys},{wheels},{motions},{},{},{},{},{},{},{},{tab},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        u8::from(window.focused), u8::from(ctx.wants_keyboard_input()), u8::from(ctx.wants_pointer_input()),
        camera.target.x, camera.target.y, camera.distance, selection.selected_entity_id.map_or(u32::MAX, |id| id.0),
        session.world().map_or(0, |world| world.current_tick), u8::from(lab.battle_paused),
        window.width(), window.height(), window.scale_factor(), ctx.pixels_per_point(), cursor.x, cursor.y,
        subject.0, subject.1.x, subject.1.y, u8::from(keyboard.pressed(KeyCode::KeyW)),
        u8::from(keyboard.pressed(KeyCode::KeyA)), u8::from(keyboard.pressed(KeyCode::KeyS)),
        u8::from(keyboard.pressed(KeyCode::KeyD)), u8::from(mouse.pressed(MouseButton::Middle)))
        .and_then(|()| {
            for rect in geometry.targets.iter().copied().chain(std::iter::once(geometry.world)) {
                if let Some(rect) = rect { write!(writer, ",{},{},{},{}", rect.min.x, rect.min.y, rect.width(), rect.height())?; }
                else { write!(writer, ",-1,-1,0,0")?; }
            }
            write!(writer, ",{cursors},{},{subject_unit}", catalog.get_by_slug(&lab.archetype_name).map_or(u16::MAX, |actor| actor.unit_type_id.0))?;
            key_delivery.write(writer, &keyboard)?;
            button_delivery.write(writer)?;
            writeln!(writer)?;
            writer.flush()
        });
    if let Err(error) = result {
        if !failed.swap(true, Ordering::AcqRel) {
            error!("Native input evidence failed: {error}");
            exit.send(AppExit::error());
        }
    }
}
