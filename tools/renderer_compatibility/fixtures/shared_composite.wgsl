struct Payload { values: array<vec4<f32>, 2>, matrix: mat2x2<f32> }
@group(0) @binding(0) var<storage, read_write> data: array<Payload>;
var<workgroup> workgroup_values: array<Payload, 4>;
var<private> private_value: Payload;
fn shifted(value: Payload) -> Payload {
    var result = value;
    result.values[0] += vec4<f32>(1.0);
    return result;
}
@compute @workgroup_size(4)
fn main(@builtin(local_invocation_index) i: u32) {
    workgroup_values[i] = data[i];
    workgroupBarrier();
    private_value = shifted(workgroup_values[(i + 1u) % 4u]);
    data[i] = private_value;
}
