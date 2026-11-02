//! Optional passive pipeline evidence for native UI verification. This recorder
//! reads the real published snapshots and ECS poses; it never submits commands,
//! steps simulation, changes selection, or substitutes scene/media state.

use bevy::prelude::*;
use client::battle_pipeline::{driver::AdmissionOutcome, session::BattleSession};
use sim_types::{BubbleRenderInstance, EntityId};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Resource)]
struct Trace {
    writer: BufWriter<File>,
    frame: u64,
    generation: Option<u64>,
    actor: Option<EntityId>,
    changes: u64,
    failed: Arc<AtomicBool>,
    last_outcome: Option<(u64, u32, usize, u8, bool)>,
    left_down: u64,
    left_up: u64,
    foreign_left: u64,
}

pub(super) fn install(app: &mut App, path: &Path, failed: Arc<AtomicBool>) -> std::io::Result<()> {
    let mut writer = BufWriter::with_capacity(
        64 * 1024,
        File::options().write(true).create_new(true).open(path)?,
    );
    writeln!(writer, "frame,delta_ns,generation,world_tick,current_tick,presented_tick,world_checksum,current_checksum,alpha,debt_ns,paused,ready,pending,outcomes,event_pending,notice_sequence,notice_tick,notice_index,notice_player,notice_ok,tracking_changes,old_actor,actor,unit,owner,previous_x,previous_y,previous_z,current_x,current_y,current_z,published_x,published_y,published_z,transform_x,transform_y,transform_z,window_x,window_y,instance_x,instance_y,instance_z,pending_identity,left_down,left_up,foreign_left,render_ready,terrain_assets_ready")?;
    writer.flush()?;
    app.insert_resource(Trace {
        writer,
        frame: 0,
        generation: None,
        actor: None,
        changes: 0,
        failed,
        last_outcome: None,
        left_down: 0,
        left_up: 0,
        foreign_left: 0,
    })
    .add_systems(Last, record);
    Ok(())
}

fn output(snapshot: &sim_types::RenderSnapshot, actor: EntityId) -> Option<&BubbleRenderInstance> {
    snapshot
        .bubbles
        .binary_search_by_key(&actor, |bubble| bubble.entity_id)
        .ok()
        .map(|index| &snapshot.bubbles[index])
}

fn observe_identity(trace: &mut Trace, session: &BattleSession) -> Option<EntityId> {
    let generation = session.commands.generation().map(|value| value.value());
    let old_actor = trace.actor;
    if generation != trace.generation {
        trace.generation = generation;
        trace.actor = None;
        trace.last_outcome = None;
    }
    if trace
        .actor
        .is_some_and(|actor| output(session.frames.presented(), actor).is_none())
    {
        trace.actor = None;
    }
    if trace.actor.is_none() {
        trace.actor = session
            .frames
            .presented()
            .bubbles
            .iter()
            .find(|bubble| {
                bubble
                    .velocity
                    .iter()
                    .any(|value| value.abs() > f32::EPSILON)
            })
            .map(|bubble| bubble.entity_id);
    }
    if trace.actor != old_actor {
        trace.changes += 1;
    }
    for notice in &session.notices {
        let identity = match &notice.outcome {
            AdmissionOutcome::Movement(value) => (
                notice.sequence.0,
                value.tick.0,
                value.command_index,
                value.player.0,
                value.result.is_ok(),
            ),
            AdmissionOutcome::Production(value) => (
                notice.sequence.0,
                value.tick.0,
                value.command_index,
                value.player.0,
                value.result.is_ok(),
            ),
        };
        trace.last_outcome = Some(identity);
    }
    old_actor
}

fn record(
    mut trace: ResMut<'_, Trace>,
    time: Res<'_, Time>,
    session: Res<'_, BattleSession>,
    lab: Res<'_, client::LabState>,
    subjects: Query<
        '_,
        '_,
        (
            &client::LiveBattleBubble,
            &client::BubbleInstance,
            &GlobalTransform,
        ),
    >,
    cameras: Query<'_, '_, (&Camera, &GlobalTransform), With<Camera3d>>,
    mut buttons: EventReader<'_, '_, bevy::input::mouse::MouseButtonInput>,
    windows: Query<'_, '_, Entity, With<bevy::window::PrimaryWindow>>,
    render_status: Res<'_, client::runtime_battle::BattleRenderStatus>,
    terrain_handle: Option<Res<'_, client::terrain::TerrainMaterialHandle>>,
    terrain_materials: Res<'_, Assets<client::terrain::TerrainMaterial>>,
    images: Res<'_, Assets<Image>>,
    map: Option<Res<'_, client::runtime_battle::MapPresentation>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    for event in buttons
        .read()
        .filter(|event| event.button == MouseButton::Left)
    {
        if windows.get_single().ok() != Some(event.window) {
            trace.foreign_left += 1;
        } else if event.state.is_pressed() {
            trace.left_down += 1;
        } else {
            trace.left_up += 1;
        }
    }
    let terrain_ready = map
        .as_ref()
        .is_some_and(|map| map.sprite_images.iter().all(|image| images.contains(image)))
        && terrain_handle
            .as_ref()
            .and_then(|handle| terrain_materials.get(&handle.0))
            .is_some_and(|material| {
                [
                    &material.extension.albedo,
                    &material.extension.normal,
                    &material.extension.orm,
                    &material.extension.splat,
                    &material.extension.overlay,
                ]
                .iter()
                .all(|image| images.contains(*image))
            });
    let generation = session.commands.generation().map(|value| value.value());
    let old_actor = observe_identity(&mut trace, &session);
    let published = trace
        .actor
        .and_then(|actor| output(session.frames.presented(), actor));
    let current = trace
        .actor
        .and_then(|actor| output(session.frames.current(), actor));
    let previous = trace.actor.and_then(|actor| {
        session
            .frames
            .previous()
            .and_then(|snapshot| output(snapshot, actor))
    });
    let actual = trace
        .actor
        .and_then(|actor| subjects.iter().find(|(live, _, _)| live.entity_id == actor));
    let projection = actual.and_then(|(_, _, position)| {
        cameras.get_single().ok().and_then(|(camera, transform)| {
            client::input_policy::world_to_window(camera, transform, position.translation())
        })
    });
    let pose = |bubble: Option<&BubbleRenderInstance>| {
        bubble.map_or([f32::NAN; 3], |bubble| bubble.world_pos)
    };
    let [px, py, pz] = pose(previous);
    let [cx, cy, cz] = pose(current);
    let [vx, vy, vz] = pose(published);
    let [tx, ty, tz] = actual.map_or([f32::NAN; 3], |(_, _, position)| {
        position.translation().to_array()
    });
    let point = projection.unwrap_or(Vec2::splat(f32::NAN));
    let [ix, iy, iz] = actual.map_or([f32::NAN; 3], |(_, instance, _)| instance.0.world_pos);
    let (sequence, tick, index, player, accepted) =
        trace.last_outcome.unwrap_or((0, 0, 0, u8::MAX, false));
    let world = session.world();
    trace.frame += 1;
    let frame = trace.frame;
    let changes = trace.changes;
    let actor = trace.actor.map_or(u32::MAX, |actor| actor.0);
    let result=write!(trace.writer,"{frame},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{sequence},{tick},{index},{player},{},{changes},{},{actor},{},{},{px},{py},{pz},{cx},{cy},{cz},{vx},{vy},{vz},{tx},{ty},{tz},{},{},{ix},{iy},{iz},",
        time.delta().as_nanos(),generation.unwrap_or(0),world.map_or(0,|world|world.current_tick),session.frames.current().tick.0,session.frames.presented().tick.0,
        world.map_or(0,sim_core::SimWorld::compute_checksum),session.frames.current().checksum,session.clock.fraction(),session.clock.debt().as_nanos(),
        u8::from(lab.battle_paused),u8::from(session.is_ready()),session.commands.pending_len(),session.outcome_count,session.frames.pending_event_count(),u8::from(accepted),old_actor.map_or(u32::MAX,|actor|actor.0),
        published.map_or(u16::MAX,|actor|actor.unit_type.0),published.map_or(u8::MAX,|actor|actor.owner.0),point.x,point.y).and_then(|()| {
        for (sequence, command) in session.commands.pending_commands() {
            write!(trace.writer,"{}:{}:",sequence.0,command.player_id.0)?;
            for entity in &command.entities { write!(trace.writer,"{}+",entity.0)?; }
            write!(trace.writer,";")?;
        }
        let (down,up,foreign)=(trace.left_down,trace.left_up,trace.foreign_left);
        writeln!(trace.writer,",{down},{up},{foreign},{},{}",u8::from(render_status.is_ready()),u8::from(terrain_ready))?;
        trace.writer.flush()
    });
    if let Err(error) = result {
        if !trace.failed.swap(true, Ordering::AcqRel) {
            error!("Native pipeline evidence failed: {error}");
            exit.send(AppExit::error());
        }
    }
}

#[cfg(test)]
mod tests;
