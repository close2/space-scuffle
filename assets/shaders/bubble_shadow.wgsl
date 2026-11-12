// assets/shaders/bubble_shadow.wgsl
// The prepass vertex stage of the bubble material (client::bubble_shader::BubbleDomeMaterial),
// used where a bubble is drawn into a prepass: in practice the shadow maps, since a blended
// bubble takes no part in the camera's depth prepass. It is bevy_pbr's prepass vertex
// (prepass.wgsl) without skinning or morph targets, which bubbles never have, plus the effect
// shell (plan 2026-11-09-effect-visualization WP8): the caster sinks with the bubble and
// shrinks towards its centre by the shell's shadow lane, so a unit's shadow fades as it digs in
// or cloaks and comes back as it emerges or uncloaks, never popping. With a neutral shell it
// computes exactly what Bevy's does.

#import bevy_pbr::{
    mesh_functions,
    prepass_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

// client::bubble_shader::ShellUniformGpu (the same binding as in bubble_dome.wgsl).
struct ShellUniform {
    tint: vec4<f32>,
    rim: vec4<f32>,
    shape: vec4<f32>,            // sink (world units), veil, soil height, ground y
    driven: vec4<f32>,           // driven, matte, shadow taken away, padding
};

@group(2) @binding(4) var<uniform> shell: ShellUniform;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let fade = clamp(shell.driven.z, 0.0, 1.0);
    if (fade > 0.0 || shell.shape.x > 0.0) {
        let centre = world_from_local[3].xyz;
        world = vec4<f32>(centre + (world.xyz - centre) * (1.0 - fade), world.w);
        world.y -= shell.shape.x;
    }
    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);
#ifdef DEPTH_CLAMP_ORTHO
    out.clip_position_unclamped = out.position;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex.instance_index);
#endif
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef MOTION_VECTOR_PREPASS
    let previous_world_from_local = mesh_functions::get_previous_world_from_local(vertex.instance_index);
    out.previous_world_position = mesh_functions::mesh_position_local_to_world(previous_world_from_local, vec4<f32>(vertex.position, 1.0));
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}
