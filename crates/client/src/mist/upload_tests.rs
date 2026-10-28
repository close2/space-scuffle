//! Actual-buffer publication, delayed callback, generation and two-view ordering checks.
//!
//! Mapping completes on the actual device before readiness delivery is deliberately held.
//! Tests run the production packet selection/copy/acknowledgment and phase correction.
//! Phase items test queue semantics only; real native captures cover rasterization.

use super::*;
use crate::mist::gpu::{EmitterHeaderGpu, PacketLayout, Publication};
use bevy::ecs::system::RunSystemOnce;
use bevy::render::render_phase::{
    sort_phase_system, DrawFunctions, PhaseItemExtraIndex, RenderCommandState,
};
use bevy::render::render_resource::{CachedRenderPipelineId, CommandEncoderDescriptor, Maintain};
use bevy::render::renderer::RenderQueue;
use std::sync::Mutex;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn submit_selected(
    state: &mut UploadState,
    device: &RenderDevice,
    queue: &RenderQueue,
) -> TestResult {
    let selected = state.selected.ok_or("packet was not selected")?;
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("actual mist delayed-publication test"),
    });
    record_selected_copy(state, selected, &mut encoder);
    queue.submit([encoder.finish()]);
    acknowledge_submission_inner(state);
    assert!(!state.failed);
    Ok(())
}

fn read_packet_bytes(
    state: &UploadState,
    device: &RenderDevice,
    queue: &RenderQueue,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let layout = state.resources.layout;
    let bytes = layout.total_bytes;
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("actual published mist packet readback"),
        size: bytes,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("actual packet readback"),
    });
    encoder.copy_buffer_to_buffer(&state.resources.headers, 0, &buffer, 0, layout.header_bytes);
    encoder.copy_buffer_to_buffer(
        &state.resources.samples,
        0,
        &buffer,
        layout.header_bytes,
        layout.sample_bytes,
    );
    encoder.copy_buffer_to_buffer(
        &state.resources.frame,
        0,
        &buffer,
        layout.frame_offset,
        size_of::<crate::mist::gpu::FrameGpu>() as u64,
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    buffer.slice(..).map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    let started = Instant::now();
    loop {
        device.poll(Maintain::Poll);
        match receiver.try_recv() {
            Ok(result) => {
                result?;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("packet readback disconnected".into())
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if started.elapsed() > Duration::from_secs(30) {
                    return Err("packet readback timeout".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    let bytes = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    Ok(bytes)
}

fn packet_bytes(packet: &Packet) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(bytemuck::cast_slice(&packet.headers));
    bytes.extend_from_slice(bytemuck::cast_slice(&packet.samples));
    bytes.extend_from_slice(bytemuck::bytes_of(&packet.frame));
    bytes
}

fn fill_generation(state: &mut UploadState, generation: u32, origin: Vec3) -> TestResult {
    state.pending.sequence = u64::from(generation);
    state.pending.frame.geometry = Vec4::new(1.4, 0.7, 0.0, 0.0);
    let value = f32::from(u16::try_from(generation)?);
    state.pending.frame.appearance.warm.w = value * 0.25;
    state.pending.frame.appearance.cool.x = value * 0.1;
    state.pending.frame.appearance.light = Vec4::new(
        super::super::lighting::default_reference_lux(),
        0.0,
        0.0,
        0.0,
    );
    for index in 0..400 {
        state.pending.headers[index] = EmitterHeaderGpu {
            control: UVec4::new(
                (index * state.resources.layout.samples_per_emitter) as u32,
                2,
                generation,
                1,
            ),
            origin_radius: origin.extend(1.0),
            capacity: UVec4::new(48, 0, 0, 0),
            ..default()
        };
        for knot in 0..2 {
            let sample = &mut state.pending.samples
                [index * state.resources.layout.samples_per_emitter + knot];
            sample.position_age = Vec4::new(
                value,
                f32::from(u16::try_from(index)?),
                f32::from(u16::try_from(knot)?),
                value * 0.1,
            );
            sample.distance =
                Vec4::new(value * 3.0 + f32::from(u16::try_from(knot)?), 0.0, 0.0, 0.0);
        }
        state.pending.slots[index] = PublishedSlot {
            generation,
            count: 2,
            origin,
            minimum: origin - Vec3::ONE,
            maximum: origin + Vec3::ONE,
        };
    }
    Ok(())
}

pub(in crate::mist) fn verify_publication_and_sorting(
    device: &RenderDevice,
    queue: &RenderQueue,
) -> TestResult {
    let style = MistStyle::default();
    let layout = PacketLayout::new(style.max_emitters, style.samples_per_emitter)?;
    let shared = Arc::new(Mutex::new(Publication::new(layout)));
    let resources = GpuResources::new(device, layout, shared.clone())?;
    let mut state = UploadState::new(device, resources, &style, false)?;
    for slot in &state.staging {
        slot.release_callback.store(false, Ordering::Release);
    }
    let created = shared
        .lock()
        .map_err(|_| "poisoned publication")?
        .accounting;
    assert_eq!(created.target_buffers, 3);
    assert_eq!(created.staging_buffers, style.gpu_staging_buffers as u64);
    for generation in 1..=3 {
        fill_generation(
            &mut state,
            generation,
            Vec3::new(0.0, 1.0, f32::from(u16::try_from(generation)?)),
        )?;
        select_packet_inner(&mut state);
        assert!(state.selected.is_some());
        assert_eq!(
            shared.lock().map_err(|_| "poisoned publication")?.sequence,
            u64::from(generation - 1),
            "selection must not acknowledge an unsubmitted packet"
        );
        submit_selected(&mut state, device, queue)?;
    }
    // A real readback drains GPU copies and map callbacks. Test gates delay only
    // delivery of CPU ownership, reproducing bounded staging backpressure.
    let retained_bytes = packet_bytes(&state.pending);
    assert_eq!(read_packet_bytes(&state, device, queue)?, retained_bytes);
    assert!(state
        .staging
        .iter()
        .all(|slot| slot.state.load(Ordering::Acquire) == HELD_MAP));
    let retained = state.published[0].origin;
    fill_generation(&mut state, 4, Vec3::new(0.0, 1.0, 18.0))?;
    select_packet_inner(&mut state);
    assert!(state.selected.is_none());
    assert!(!state.failed);
    assert_eq!(state.selected_slots()[0].generation, 3);
    assert_eq!(state.selected_slots()[0].origin, retained);
    assert_eq!(
        shared.lock().map_err(|_| "poisoned publication")?.sequence,
        3
    );
    assert_eq!(read_packet_bytes(&state, device, queue)?, retained_bytes);
    let next_bytes = packet_bytes(&state.pending);
    assert_ne!(next_bytes, retained_bytes);
    state = verify_views_and_reused_owner(state, 3, false)?;
    // Deliver one completed callback. The same allocation now publishes all400
    // replacement owners, and each actual GPU header switches as one packet.
    state.staging[0]
        .release_callback
        .store(true, Ordering::Release);
    state.staging[0].state.store(WRITABLE, Ordering::Release);
    select_packet_inner(&mut state);
    assert_eq!(state.selected, Some(0));
    assert_eq!(state.selected_slots()[0].origin.z, 18.0);
    state = verify_views_and_reused_owner(state, 4, false)?;
    submit_selected(&mut state, device, queue)?;
    assert_eq!(read_packet_bytes(&state, device, queue)?, next_bytes);
    let final_counts = shared
        .lock()
        .map_err(|_| "poisoned publication")?
        .accounting;
    assert_eq!(final_counts.target_buffers, created.target_buffers);
    assert_eq!(final_counts.staging_buffers, created.staging_buffers);
    assert_eq!(final_counts.submitted_packets, 4);
    assert_eq!(final_counts.submitted_bytes, layout.total_bytes * 4);
    assert_eq!(final_counts.staging_stalls, 1);
    state = verify_views_and_reused_owner(state, 5, true)?;
    assert!(state.failed);
    assert!(matches!(
        shared.lock().map_err(|_| "poisoned publication")?.error,
        Some(MistGpuError::StaleGeneration { .. })
    ));
    Ok(())
}

fn verify_views_and_reused_owner(
    state: UploadState,
    generation: u32,
    stale: bool,
) -> Result<UploadState, Box<dyn std::error::Error>> {
    let selected_origin = state.selected_slots()[0].origin;
    // The actual view rangefinder and Transparent3d sorter are exercised with two
    // opposing camera transforms, rather than a separate approximate sort formula.
    let mut world = World::new();
    let draw_functions = DrawFunctions::<Transparent3d>::default();
    let command = RenderCommandState::<Transparent3d, ()>::new(&mut world);
    let draw_function = draw_functions.write().add(command);
    let mist = world
        .spawn(MistGpuIdentity {
            slot: 0,
            generation,
        })
        .id();
    let shell = world.spawn_empty().id();
    let other = world.spawn_empty().id();
    let mut phases = ViewSortedRenderPhases::<Transparent3d>::default();
    let mut views = Vec::new();
    for position in [Vec3::new(0.0, 3.0, 30.0), Vec3::new(0.0, 3.0, -30.0)] {
        let transform = Transform::from_translation(position).looking_at(Vec3::ZERO, Vec3::Y);
        let view = world
            .spawn(ExtractedView {
                clip_from_view: Mat4::perspective_rh(0.8, 1.0, 0.1, 100.0),
                world_from_view: GlobalTransform::from(transform),
                clip_from_world: None,
                hdr: false,
                viewport: UVec4::new(0, 0, 1280, 720),
                color_grading: default(),
            })
            .id();
        phases.insert_or_clear(view);
        let phase = phases.get_mut(&view).ok_or("phase absent")?;
        for entity in [mist, shell, other] {
            let distance = if entity == shell {
                world
                    .get::<ExtractedView>(view)
                    .ok_or("view missing")?
                    .rangefinder3d()
                    .distance_translation(&selected_origin)
            } else {
                -500.0
            };
            phase.items.push(Transparent3d {
                distance,
                pipeline: CachedRenderPipelineId::INVALID,
                entity: (entity, entity.into()),
                draw_function,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::NONE,
            });
        }
        views.push(view);
    }
    world.insert_resource(state);
    world.insert_resource(phases);
    world.insert_resource(LightAdmission::Admitted);
    world.run_system_once(align_transparent_packet)?;
    for view_entity in &views {
        let view = world
            .get::<ExtractedView>(*view_entity)
            .ok_or("view missing")?;
        let phases = world.resource::<ViewSortedRenderPhases<Transparent3d>>();
        let phase = phases.get(view_entity).ok_or("phase missing")?;
        if stale {
            assert_eq!(
                phase.items.len(),
                2,
                "stale mist must be removed from every view"
            );
            assert!(
                phase.items.iter().any(|item| item.entity() == other),
                "unrelated transparency must remain"
            );
        } else {
            let mist_item = phase
                .items
                .iter()
                .find(|item| item.entity() == mist)
                .ok_or("mist missing")?;
            assert_eq!(
                mist_item.distance,
                view.rangefinder3d().distance_translation(&selected_origin)
            );
            assert_eq!(
                phase
                    .items
                    .iter()
                    .find(|item| item.entity() == other)
                    .ok_or("other missing")?
                    .distance,
                -500.0
            );
        }
    }
    world.run_system_once(sort_phase_system::<Transparent3d>)?;
    for view in &views {
        let phases = world.resource::<ViewSortedRenderPhases<Transparent3d>>();
        let phase = phases.get(view).ok_or("phase missing")?;
        if !stale {
            let mist_index = phase
                .items
                .iter()
                .position(|item| item.entity() == mist)
                .ok_or("mist missing")?;
            let shell_index = phase
                .items
                .iter()
                .position(|item| item.entity() == shell)
                .ok_or("shell missing")?;
            assert!(
                mist_index < shell_index,
                "stable exact-depth tie must retain mist before its owner shell"
            );
        }
    }
    Ok(world
        .remove_resource::<UploadState>()
        .ok_or("state absent")?)
}

#[test]
fn registered_queue_chain_fixes_exact_depth_ties_without_numeric_bias() -> TestResult {
    use bevy::ecs::schedule::IntoSystemSet;
    let mut schedule = Schedule::default();
    schedule.add_systems((
        queue_material_meshes::<MistWakeMaterial>,
        queue_material_meshes::<crate::BubbleDomeMaterial>,
        aligned_queue_system(),
    ));
    let graph = schedule.graph();
    let mist_set = queue_material_meshes::<MistWakeMaterial>
        .into_system_set()
        .intern();
    let shell_set = queue_material_meshes::<crate::BubbleDomeMaterial>
        .into_system_set()
        .intern();
    let mist = graph
        .system_sets()
        .find(|(_, set, _)| set.as_dyn_eq().dyn_eq(mist_set.as_dyn_eq()))
        .map(|(id, _, _)| id)
        .ok_or("mist queue set missing")?;
    let shell = graph
        .system_sets()
        .find(|(_, set, _)| set.as_dyn_eq().dyn_eq(shell_set.as_dyn_eq()))
        .map(|(id, _, _)| id)
        .ok_or("shell queue set missing")?;
    let alignment = graph
        .systems()
        .find(|(_, system, _)| system.name().ends_with("align_transparent_packet"))
        .map(|(id, _, _)| id)
        .ok_or("alignment system missing")?;
    assert!(graph.dependency().graph().contains_edge(mist, alignment));
    assert!(graph.dependency().graph().contains_edge(alignment, shell));
    Ok(())
}

#[test]
fn empty_named_set_preserves_queue_dependencies_without_duplicate_systems() -> TestResult {
    use bevy::ecs::schedule::IntoSystemSet;
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Bridge;
    #[derive(Resource, Default)]
    struct Calls(Vec<u8>);
    fn first(mut calls: ResMut<'_, Calls>) {
        calls.0.push(1);
    }
    fn second(mut calls: ResMut<'_, Calls>) {
        calls.0.push(2);
    }
    let mut world = World::new();
    world.init_resource::<Calls>();
    let mut schedule = Schedule::default();
    schedule.add_systems((second, first));
    schedule.configure_sets(Bridge.after(first).before(second));
    for _ in 0..32 {
        world.resource_mut::<Calls>().0.clear();
        schedule.run(&mut world);
        assert_eq!(world.resource::<Calls>().0, [1, 2]);
    }
    let mut actual = Schedule::default();
    actual.add_systems((
        queue_material_meshes::<crate::BubbleDomeMaterial>,
        queue_material_meshes::<MistWakeMaterial>,
    ));
    actual.configure_sets(
        Bridge
            .after(queue_material_meshes::<MistWakeMaterial>)
            .before(queue_material_meshes::<crate::BubbleDomeMaterial>),
    );
    let graph = actual.graph();
    let find_set = |set: bevy::ecs::schedule::InternedSystemSet| {
        graph
            .system_sets()
            .find(|(_, value, _)| value.as_dyn_eq().dyn_eq(set.as_dyn_eq()))
            .map(|(id, _, _)| id)
            .ok_or("registered queue set missing")
    };
    let mist = find_set(
        queue_material_meshes::<MistWakeMaterial>
            .into_system_set()
            .intern(),
    )?;
    let shell = find_set(
        queue_material_meshes::<crate::BubbleDomeMaterial>
            .into_system_set()
            .intern(),
    )?;
    let bridge = find_set(Bridge.intern())?;
    assert_eq!(graph.systems().count(), 2);
    assert!(graph.dependency().graph().contains_edge(mist, bridge));
    assert!(graph.dependency().graph().contains_edge(bridge, shell));
    Ok(())
}

#[test]
fn pending_or_failed_light_admission_suppresses_only_mist_without_uploader() -> TestResult {
    // Exercise the actual queue system before uploader creation, when an AppExit
    // alone would otherwise leave queued transparent draws in this render frame.
    for admission in [
        LightAdmission::Pending,
        LightAdmission::Failed(super::super::MistLightError::Illuminance(Entity::from_raw(
            99,
        ))),
    ] {
        let mut world = World::new();
        let functions = DrawFunctions::<Transparent3d>::default();
        let command = RenderCommandState::<Transparent3d, ()>::new(&mut world);
        let draw_function = functions.write().add(command);
        let mist = world
            .spawn(MistGpuIdentity {
                slot: 0,
                generation: 1,
            })
            .id();
        let unrelated = world.spawn_empty().id();
        let mut phases = ViewSortedRenderPhases::<Transparent3d>::default();
        for _ in 0..2 {
            let view = world.spawn_empty().id();
            phases.insert_or_clear(view);
            let phase = phases.get_mut(&view).ok_or("missing phase")?;
            for entity in [mist, unrelated] {
                phase.items.push(Transparent3d {
                    distance: 1.0,
                    pipeline: CachedRenderPipelineId::INVALID,
                    entity: (entity, entity.into()),
                    draw_function,
                    batch_range: 0..1,
                    extra_index: PhaseItemExtraIndex::NONE,
                });
            }
        }
        world.insert_resource(admission);
        world.insert_resource(phases);
        world.run_system_once(align_transparent_packet)?;
        for phase in world
            .resource::<ViewSortedRenderPhases<Transparent3d>>()
            .values()
        {
            assert_eq!(phase.items.len(), 1);
            assert_eq!(phase.items[0].entity(), unrelated);
        }
    }
    Ok(())
}
