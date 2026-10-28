var<workgroup> previous_mip_depth: array<array<f32, 8>, 8>;
@group(0) @binding(0) var<storage, read_write> output: array<f32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(local_invocation_id) id: vec3<u32>) {
    previous_mip_depth[id.x][id.y] = f32(id.x + 8u * id.y);
    workgroupBarrier();
    output[id.x + 8u * id.y] = previous_mip_depth[id.y][id.x];
}
