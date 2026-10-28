struct Parameters {
    scale: f32,
    direction: vec3<f32>,
    transform: mat3x3<f32>,
    pair: array<mat4x4<f32>, 2>,
}
@group(0) @binding(0) var<uniform> params: Parameters;
@group(0) @binding(1) var<storage, read_write> result: Parameters;
@compute @workgroup_size(1)
fn main() {
    var local = params;
    local.direction = local.transform * local.direction * local.scale;
    result = local;
}
