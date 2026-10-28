//! Passive production-scene evidence for Lab → battle → Lab lighting ownership.
//!
//! Native UI interaction owns transitions. This observer reads final main-world
//! visibility and the actual renderer light buffer, correlates asynchronous epochs,
//! and never modifies scene, camera, light or simulation presentation state.

use super::light_probe::{ProbeEvidence, ProbeReport, ProbeRequest};
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Explicit setup, epoch, GPU and persistence failures of the optional observer.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SceneTraceError {
    /// The actual shader/style or optional observer could not initialize.
    #[error("scene light trace setup failed: {0}")]
    Setup(String),
    /// Ordinary production scene ownership or asynchronous evidence changed unexpectedly.
    #[error("scene light trace invariant failed: {0}")]
    Invariant(&'static str),
    /// An actual prepared-light readback failed.
    #[error(transparent)]
    Probe(#[from] super::light_probe::ProbeError),
    /// Evidence could not be created or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Evidence could not be serialized.
    #[error(transparent)]
    Encode(#[from] ron::Error),
}

/// Shared terminal acknowledgment survives ordinary application teardown.
#[derive(Clone)]
pub(crate) struct Completion(Arc<Mutex<Option<Result<(), String>>>>);

impl Completion {
    /// Rejects otherwise-successful closes before all three real epochs complete.
    pub(crate) fn finish(&self) -> Result<(), SceneTraceError> {
        match self.0.lock() {
            Ok(state) => match state.as_ref() {
                Some(Ok(())) => Ok(()),
                Some(Err(message)) => Err(SceneTraceError::Setup(message.clone())),
                None => Err(SceneTraceError::Invariant(
                    "incomplete Lab/battle/Lab evidence",
                )),
            },
            Err(_) => Err(SceneTraceError::Invariant("completion mutex poisoned")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum Owner {
    Arena,
    Battle(u64),
}

#[derive(Clone, PartialEq, Serialize)]
struct Rig {
    entity: u64,
    parent: Option<u64>,
    owner: Owner,
    local_visibility: String,
    inherited_visible: bool,
    view_visible: bool,
    direction_to_light: [f32; 3],
    linear_rgb: [f32; 3],
    lux: f32,
    layers: Vec<usize>,
}

#[derive(Clone, Serialize)]
struct Inventory {
    scene: &'static str,
    camera: u64,
    camera_layers: Vec<usize>,
    rigs: Vec<Rig>,
}

#[derive(Serialize)]
struct Checkpoint<'a> {
    nonce: u64,
    inventory: &'a Inventory,
    actual_view_lights: &'a ProbeReport,
}

#[derive(Resource)]
struct Trace {
    out: PathBuf,
    started: Instant,
    index: usize,
    pending: Option<Inventory>,
    arena: Option<u64>,
    battle: Option<(u64, u64)>,
    initial_reading: Option<super::light_probe::LightReading>,
    reference: f32,
    completion: Completion,
    failed: bool,
}

/// Installs optional real-buffer observation without changing production scene state.
pub(crate) fn install(app: &mut App, out: &Path) -> Result<Completion, SceneTraceError> {
    std::fs::create_dir(out)?;
    let root = app
        .world()
        .resource::<crate::assets::ResolvedAssetsRoot>()
        .as_path()
        .to_owned();
    let style = super::MistStyle::load(&root.join("catalog/mist.ron"))
        .map_err(|error| SceneTraceError::Setup(error.to_string()))?;
    super::light_probe::install(app, &root, style.reference_illuminance_lux)
        .map_err(|error| SceneTraceError::Setup(error.to_string()))?;
    let completion = Completion(Arc::new(Mutex::new(None)));
    app.insert_resource(Trace {
        out: out.to_owned(),
        started: Instant::now(),
        index: 0,
        pending: None,
        arena: None,
        battle: None,
        initial_reading: None,
        reference: style.reference_illuminance_lux,
        completion: completion.clone(),
        failed: false,
    })
    .add_systems(Last, observe);
    Ok(completion)
}

type Rigs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DirectionalLight,
        &'static GlobalTransform,
        &'static Visibility,
        &'static InheritedVisibility,
        &'static ViewVisibility,
        Option<&'static RenderLayers>,
        Option<&'static Parent>,
        Has<crate::arena_render::ArenaMapMarker>,
    ),
>;

type Cameras<'w, 's> =
    Query<'w, 's, (Entity, &'static Camera, Option<&'static RenderLayers>), With<crate::RtsCamera>>;

fn layers_match(layers: Option<&RenderLayers>, expected: &[usize]) -> bool {
    layers
        .unwrap_or(&RenderLayers::default())
        .iter()
        .eq(expected.iter().copied())
}

fn inspect(
    rigs: &Rigs<'_, '_>,
    roots: &Query<'_, '_, (), With<crate::runtime_battle::BattleSceneRoot>>,
    cameras: &Cameras<'_, '_>,
    scene: &'static str,
) -> Result<Inventory, SceneTraceError> {
    let (camera, view, layers) = cameras
        .get_single()
        .map_err(|_| SceneTraceError::Invariant("exactly one production camera required"))?;
    if !view.is_active {
        return Err(SceneTraceError::Invariant("production camera inactive"));
    }
    let count = rigs.iter().count();
    if count > 64 {
        return Err(SceneTraceError::Invariant(
            "directional rig inventory capacity exceeded",
        ));
    }
    let mut inventory = Inventory {
        scene,
        camera: camera.to_bits(),
        camera_layers: layers.unwrap_or(&RenderLayers::default()).iter().collect(),
        rigs: Vec::with_capacity(count),
    };
    for (entity, light, transform, visibility, inherited, visible, layers, parent, arena) in rigs {
        let owner = if arena {
            Owner::Arena
        } else if let Some(parent) = parent.filter(|parent| roots.contains(parent.get())) {
            Owner::Battle(parent.get().to_bits())
        } else {
            return Err(SceneTraceError::Invariant(
                "unaccounted directional rig owner",
            ));
        };
        let rgb = light.color.to_linear();
        inventory.rigs.push(Rig {
            entity: entity.to_bits(),
            parent: parent.map(|p| p.get().to_bits()),
            owner,
            local_visibility: format!("{visibility:?}"),
            inherited_visible: inherited.get(),
            view_visible: visible.get(),
            direction_to_light: transform.back().to_array(),
            linear_rgb: [rgb.red, rgb.green, rgb.blue],
            lux: light.illuminance,
            layers: layers.unwrap_or(&RenderLayers::default()).iter().collect(),
        });
    }
    inventory.rigs.sort_by_key(|rig| rig.entity);
    Ok(inventory)
}

fn unchanged(inventory: &Inventory, rigs: &Rigs<'_, '_>, cameras: &Cameras<'_, '_>) -> bool {
    let Ok((camera, view, layers)) = cameras.get_single() else {
        return false;
    };
    if camera.to_bits() != inventory.camera
        || !view.is_active
        || !layers_match(layers, &inventory.camera_layers)
        || rigs.iter().count() != inventory.rigs.len()
    {
        return false;
    }
    inventory.rigs.iter().all(|old| {
        let Ok((_, light, transform, visibility, inherited, visible, layers, parent, arena)) =
            rigs.get(Entity::from_bits(old.entity))
        else {
            return false;
        };
        let rgb = light.color.to_linear();
        let local = match visibility {
            Visibility::Inherited => "Inherited",
            Visibility::Hidden => "Hidden",
            Visibility::Visible => "Visible",
        };
        old.local_visibility == local
            && old.inherited_visible == inherited.get()
            && old.view_visible == visible.get()
            && old.parent == parent.map(|p| p.get().to_bits())
            && arena == (old.owner == Owner::Arena)
            && old.direction_to_light.map(f32::to_bits)
                == transform.back().to_array().map(f32::to_bits)
            && old.linear_rgb.map(f32::to_bits) == [rgb.red, rgb.green, rgb.blue].map(f32::to_bits)
            && old.lux.to_bits() == light.illuminance.to_bits()
            && layers_match(layers, &old.layers)
    })
}

fn admit_inventory(trace: &mut Trace, inventory: &Inventory) -> Result<(), SceneTraceError> {
    let arena: Vec<_> = inventory
        .rigs
        .iter()
        .filter(|rig| rig.owner == Owner::Arena)
        .collect();
    let battle: Vec<_> = inventory
        .rigs
        .iter()
        .filter(|rig| matches!(rig.owner, Owner::Battle(_)))
        .collect();
    if arena.len() != 1 || battle.len() != usize::from(trace.index != 0) {
        return Err(SceneTraceError::Invariant(
            "unexpected ordinary scene rig count",
        ));
    }
    let arena = arena[0];
    if trace.arena.is_some_and(|id| id != arena.entity) {
        return Err(SceneTraceError::Invariant("original arena rig replaced"));
    }
    trace.arena = Some(arena.entity);
    if arena.view_visible != (trace.index != 1) || arena.inherited_visible != (trace.index != 1) {
        return Err(SceneTraceError::Invariant(
            "arena ownership visibility mismatch",
        ));
    }
    if let Some(battle) = battle.first() {
        let Owner::Battle(root) = battle.owner else {
            return Err(SceneTraceError::Invariant("battle owner missing"));
        };
        let pair = (root, battle.entity);
        if trace.battle.is_some_and(|expected| expected != pair) {
            return Err(SceneTraceError::Invariant("committed battle rig replaced"));
        }
        trace.battle = Some(pair);
        if battle.view_visible != (trace.index == 1)
            || battle.inherited_visible != (trace.index == 1)
        {
            return Err(SceneTraceError::Invariant(
                "battle ownership visibility mismatch",
            ));
        }
    }
    Ok(())
}

fn verify_reading(
    inventory: &Inventory,
    report: &ProbeReport,
    reference: f32,
) -> Result<(), SceneTraceError> {
    if report.views.len() != 1 || report.views[0].camera != inventory.camera {
        return Err(SceneTraceError::Invariant("readback camera mismatch"));
    }
    let reading = &report.views[0].reading;
    let mut visible = inventory.rigs.iter().filter(|rig| rig.view_visible);
    let rig = visible
        .next()
        .ok_or(SceneTraceError::Invariant("no visible scene rig"))?;
    if visible.next().is_some()
        || reading.counts[..2] != [1, 1]
        || !rig
            .layers
            .iter()
            .any(|layer| inventory.camera_layers.contains(layer))
    {
        return Err(SceneTraceError::Invariant(
            "prepared scene eligibility mismatch",
        ));
    }
    let close = |actual: f32, expected: f32| {
        actual.is_finite() && (actual - expected).abs() <= 0.000_02 * expected.abs().max(1.0)
    };
    for channel in 0..3 {
        if !close(
            reading.colors[0][channel],
            rig.linear_rgb[channel] * rig.lux,
        ) || !close(
            reading.directions[0][channel],
            rig.direction_to_light[channel],
        ) {
            return Err(SceneTraceError::Invariant(
                "prepared light differs from actual scene rig",
            ));
        }
        for axis in 0..3 {
            let expected = rig.linear_rgb[channel] * rig.lux / reference
                * rig.direction_to_light[axis].max(0.0);
            if !close(reading.axes[axis][channel], expected) {
                return Err(SceneTraceError::Invariant(
                    "actual scene direct-light response mismatch",
                ));
            }
        }
    }
    if reading.directions[0][3] != 0.0 {
        return Err(SceneTraceError::Invariant(
            "visible scene light unexpectedly skipped",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn observe(
    mut trace: ResMut<'_, Trace>,
    lab: Res<'_, crate::LabState>,
    rigs: Rigs<'_, '_>,
    roots: Query<'_, '_, (), With<crate::runtime_battle::BattleSceneRoot>>,
    cameras: Cameras<'_, '_>,
    evidence: Res<'_, ProbeEvidence>,
    mut request: ResMut<'_, ProbeRequest>,
    mut exit: EventWriter<'_, AppExit>,
) {
    if trace.failed || trace.index == 3 {
        return;
    }
    let result = (|| -> Result<(), SceneTraceError> {
        if trace.started.elapsed() > Duration::from_secs(180) {
            return Err(SceneTraceError::Invariant(
                "180-second scene trace deadline exceeded",
            ));
        }
        let expected_tab = if trace.index == 1 {
            crate::LabTab::LiveBattle
        } else {
            crate::LabTab::VisualLab
        };
        if lab.current_tab != expected_tab {
            if trace.pending.is_some() {
                return Err(SceneTraceError::Invariant(
                    "tab changed before readback acknowledgment",
                ));
            }
            return Ok(());
        }
        let nonce = u64::try_from(trace.index)
            .map_err(|_| SceneTraceError::Invariant("checkpoint index overflow"))?
            + 1;
        if trace.pending.is_none() {
            if trace.index == 1 && roots.is_empty() {
                return Ok(());
            }
            if rigs.is_empty() {
                return Ok(());
            }
            let inventory = inspect(
                &rigs,
                &roots,
                &cameras,
                if trace.index == 1 { "battle" } else { "lab" },
            )?;
            admit_inventory(&mut trace, &inventory)?;
            request.nonce = nonce;
            request.views = [Some(Entity::from_bits(inventory.camera)), None];
            trace.pending = Some(inventory);
        }
        let inventory = trace
            .pending
            .as_ref()
            .ok_or(SceneTraceError::Invariant("pending inventory missing"))?;
        if !unchanged(inventory, &rigs, &cameras) {
            return Err(SceneTraceError::Invariant(
                "light/camera epoch changed before readback acknowledgment",
            ));
        }
        let Some(report) = evidence.read(nonce)? else {
            return Ok(());
        };
        verify_reading(inventory, &report, trace.reference)?;
        if trace.index == 2 {
            let initial = trace
                .initial_reading
                .as_ref()
                .ok_or(SceneTraceError::Invariant("initial Lab reading missing"))?;
            if bytemuck::bytes_of(initial) != bytemuck::bytes_of(&report.views[0].reading) {
                return Err(SceneTraceError::Invariant(
                    "restored Lab prepared light changed",
                ));
            }
        }
        let checkpoint = Checkpoint {
            nonce,
            inventory,
            actual_view_lights: &report,
        };
        let text = ron::ser::to_string_pretty(&checkpoint, ron::ser::PrettyConfig::default())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(trace.out.join(format!("scene-light-{nonce}.ron")))?;
        file.write_all(text.as_bytes())?;
        if trace.index == 0 {
            trace.initial_reading = Some(report.views[0].reading);
        }
        trace.pending = None;
        trace.index += 1;
        info!("ACTUAL SCENE LIGHT CHECKPOINT {nonce} COMPLETE");
        if trace.index == 3 {
            *trace
                .completion
                .0
                .lock()
                .map_err(|_| SceneTraceError::Invariant("completion mutex poisoned"))? =
                Some(Ok(()));
        }
        Ok(())
    })();
    if let Err(error) = result {
        trace.failed = true;
        error!("{error}");
        if let Ok(mut state) = trace.completion.0.lock() {
            *state = Some(Err(error.to_string()));
        }
        exit.send(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initial() -> (Trace, Inventory) {
        let completion = Completion(Arc::new(Mutex::new(None)));
        let trace = Trace {
            out: PathBuf::new(),
            started: Instant::now(),
            index: 0,
            pending: None,
            arena: None,
            battle: None,
            initial_reading: None,
            reference: 10000.0,
            completion,
            failed: false,
        };
        let inventory = Inventory {
            scene: "lab",
            camera: 9,
            camera_layers: vec![0],
            rigs: vec![Rig {
                entity: 1,
                parent: None,
                owner: Owner::Arena,
                local_visibility: "Visible".into(),
                inherited_visible: true,
                view_visible: true,
                direction_to_light: [0.0, 1.0, 0.0],
                linear_rgb: [1.0; 3],
                lux: 14000.0,
                layers: vec![0],
            }],
        };
        (trace, inventory)
    }

    #[test]
    fn admission_follows_lazy_battle_creation_and_exact_returned_owners(
    ) -> Result<(), SceneTraceError> {
        let (mut trace, mut inventory) = initial();
        admit_inventory(&mut trace, &inventory)?;
        trace.index = 1;
        inventory.scene = "battle";
        inventory.rigs[0].view_visible = false;
        inventory.rigs[0].inherited_visible = false;
        let mut battle = inventory.rigs[0].clone();
        battle.entity = 2;
        battle.parent = Some(3);
        battle.owner = Owner::Battle(3);
        battle.view_visible = true;
        battle.inherited_visible = true;
        inventory.rigs.push(battle);
        admit_inventory(&mut trace, &inventory)?;
        trace.index = 2;
        inventory.scene = "lab";
        inventory.rigs[0].view_visible = true;
        inventory.rigs[0].inherited_visible = true;
        inventory.rigs[1].view_visible = false;
        inventory.rigs[1].inherited_visible = false;
        admit_inventory(&mut trace, &inventory)?;
        inventory.rigs[1].entity = 4;
        assert!(admit_inventory(&mut trace, &inventory).is_err());
        Ok(())
    }

    #[test]
    fn reading_guard_requires_actual_scene_rgb_direction_and_eligibility(
    ) -> Result<(), SceneTraceError> {
        let (_, inventory) = initial();
        let mut reading = super::super::light_probe::LightReading::zeroed();
        reading.counts = [1, 1, 0, 0];
        reading.colors[0] = [14000.0, 14000.0, 14000.0, 1.0];
        reading.directions[0] = [0.0, 1.0, 0.0, 0.0];
        reading.axes[1] = [1.4, 1.4, 1.4, 0.0];
        let mut report = ProbeReport {
            nonce: 1,
            views: vec![super::super::light_probe::ViewReading {
                camera: 9,
                offset: 0,
                buffer: "unit-test-contract-only".into(),
                buffer_bytes: 0,
                queued_mist: 0,
                reading,
            }],
        };
        verify_reading(&inventory, &report, 10000.0)?;
        report.views[0].reading.counts[1] = 0;
        assert!(verify_reading(&inventory, &report, 10000.0).is_err());
        report.views[0].reading.counts[1] = 1;
        report.views[0].reading.colors[0][0] = 10000.0;
        assert!(verify_reading(&inventory, &report, 10000.0).is_err());
        Ok(())
    }

    #[test]
    fn normal_close_requires_all_three_acknowledged_epochs(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let completion = Completion(Arc::new(Mutex::new(None)));
        assert!(completion.finish().is_err());
        *completion.0.lock().map_err(|_| "poisoned completion")? =
            Some(Err("actual readback failed".into()));
        assert!(completion.finish().is_err());
        *completion.0.lock().map_err(|_| "poisoned completion")? = Some(Ok(()));
        assert!(completion.finish().is_ok());
        Ok(())
    }

    use bytemuck::Zeroable;
}
