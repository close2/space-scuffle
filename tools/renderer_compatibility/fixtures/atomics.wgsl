@group(0) @binding(0) var<storage, read_write> counter: atomic<u32>;
var<workgroup> shared_counter: atomic<u32>;
@compute @workgroup_size(4)
fn main(@builtin(local_invocation_index) i: u32) {
    atomicStore(&shared_counter, i);
    workgroupBarrier();
    let a = atomicAdd(&counter, atomicLoad(&shared_counter));
    let b = atomicExchange(&shared_counter, a);
    let c = atomicCompareExchangeWeak(&counter, b, i);
    atomicStore(&counter, c.old_value);
    storageBarrier();
}
