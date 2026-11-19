// Effect decal aging (plan 2026-11-19-retained-effect-instances D-5, D-8): the compute pass that
// turns the retained decal rows into the frame's decal boxes (client::effects::render,
// `EffectDecalAgeNode`). A port of the Rust reference `client::effects::decal_table::age_decal`
// (`decal_clock`, then `decal_at`), expression for expression and in the same order, bound to it
// by the record parity check of `decal_age_gpu_tests`.
//
// Two entry points run in one compute pass, before the main opaque pass:
// - `apply_patches`: thread i copies patch i's record into its row (rows of one patch list are
//   unique, so the order of the writes does not matter);
// - `age`: thread i ages row `order[i]` at `now` for the camera and writes decal i, which
//   `decal.wgsl` draws unchanged. A row that draws nothing (not started, ended, faded or eroded
//   out, below the level-of-detail threshold) is written as the zero decal: a box without
//   extent, which rasterises nothing.
//
// Times arrive split into two f32 (hi, lo) because WGSL has no f64; seconds since an event are
// `(now_hi - at_hi) + (now_lo - at_lo)`, the Rust reference's `split_seconds`.
#import space_scuffle::effects::common::DecalInstance

// One retained row: `client::effects::decal_table::GpuDecalRecord` byte for byte.
struct DecalRecord {
    base: DecalInstance,
    birth_hi: f32,
    birth_lo: f32,
    released_hi: f32,
    released_lo: f32,
    detail_retired_hi: f32,
    detail_retired_lo: f32,
    marks_retired_hi: f32,
    marks_retired_lo: f32,
    start: f32,
    delay: f32,
    span: f32,
    exit: f32,
    strength: f32,
    size: f32,
    size_strength: f32,
    jitter: f32,
    depth: f32,
    width: f32,
    flipbook_first: u32,
    flipbook_frames: u32,
    curve: u32,
    size_curve: u32,
    erosion_curve: u32,
    ramp_curve: u32,
    flags: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

// One row written this frame: `client::effects::render::GpuDecalPatch`.
struct DecalPatch {
    row: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    record: DecalRecord,
}

// One curve key: `client::effects::decal_table::GpuCurveKey`.
struct CurveKey {
    time: f32,
    value: f32,
    easing: u32,
    pad: u32,
}

// The frame's inputs: `client::effects::render::DecalAgeUniform`.
struct DecalAgeParams {
    now_hi: f32,
    now_lo: f32,
    min_screen_px: f32,
    has_camera: u32,
    camera_position: vec3<f32>,
    half_tan: f32,
    camera_forward: vec3<f32>,
    viewport_px: f32,
    patches: u32,
    rows: u32,
    curves: u32,
    zero_bits: u32,
    retire_secs: f32,
    mark_retire_secs: f32,
    pad1: u32,
    pad2: u32,
}

// `client::effects::decal_table::record_flags`.
const FLAG_HELD: u32 = 1u;
const FLAG_DETAIL: u32 = 2u;
const FLAG_RELEASED: u32 = 4u;
const FLAG_DETAIL_RETIRED: u32 = 8u;
const FLAG_MARKS_RETIRED: u32 = 16u;
const FLAG_FLIPBOOK: u32 = 32u;
// `client::effects::decal_table::NO_CURVE`.
const NO_CURVE: u32 = 0xFFFFFFFFu;
// `client::effects::curve::easing_code`.
const EASE_STEP: u32 = 1u;
const EASE_IN_QUAD: u32 = 2u;
const EASE_OUT_QUAD: u32 = 3u;
const EASE_OUT_CUBIC: u32 = 4u;
const EASE_IN_OUT: u32 = 5u;
// Shortest exit a released layer erodes out over, seconds (`exit.max(1e-3)` in Rust).
const MIN_EXIT: f32 = 1e-3;
// Nearest depth a projected size is measured at (`projected_px_with`).
const MIN_DISTANCE: f32 = 1e-3;
// `f32::EPSILON`.
const F32_EPSILON: f32 = 1.1920929e-7;
// Veltkamp's splitter for f32: 2^12 + 1.
const VELTKAMP: f32 = 4097.0;
// Threads per workgroup of both entry points.
const WORKGROUP: u32 = 64u;

@group(0) @binding(0) var<uniform> params: DecalAgeParams;
@group(0) @binding(1) var<storage, read_write> rows: array<DecalRecord>;
@group(0) @binding(2) var<storage, read> order: array<u32>;
@group(0) @binding(3) var<storage, read> patches: array<DecalPatch>;
@group(0) @binding(4) var<storage, read> curve_ranges: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read> curve_keys: array<CurveKey>;
@group(0) @binding(6) var<storage, read_write> decals: array<DecalInstance>;

// `x` unchanged, through an integer operation with a zero the compiler cannot see (the uniform's
// `zero_bits`): a barrier that keeps the driver's float optimizer, which may treat float
// arithmetic as reassociable, from simplifying across it (`a - (a / b) * b` to 0, say).
fn opaque(x: f32) -> f32 {
    return bitcast<f32>(bitcast<u32>(x) ^ params.zero_bits);
}

// `x` as the sum of two halves of 12 significant bits each (Veltkamp's split), whose products
// are exact in f32.
fn split(x: f32) -> vec2<f32> {
    let c = opaque(VELTKAMP * x);
    let hi = opaque(c - opaque(c - x));
    return vec2<f32>(hi, opaque(x - hi));
}

// `a - q * b`, exactly when `q` is near `a / b` (Dekker's two-product: `q * b` is `p` plus the
// exact error `e` summed from the halves' products; `a - p` is exact by Sterbenz's lemma). Only
// `*` and `-`, each correctly rounded in WGSL: unlike `fma`, which a driver may split into a
// rounded product and a sum (radv on the RX 580 does), and which then loses the residual.
fn residual(a: f32, q: f32, b: f32) -> f32 {
    let p = opaque(q * b);
    let qs = split(q);
    let bs = split(b);
    let e = opaque(
        opaque(opaque(opaque(qs.x * bs.x) - p) + opaque(qs.x * bs.y)) + opaque(qs.y * bs.x),
    ) + opaque(qs.y * bs.y);
    return opaque(a - p) - e;
}

// `a / b` rounded as Rust's IEEE division rounds it. WGSL's `/` may be 2.5 ulp off (a reciprocal
// and a product on radv), and a life one ulp off moves a steep curve's value by tens of ulp; one
// residual step on the opaque quotient makes it correctly rounded but for rare ties, so the
// port keeps the reference's bits (D-10 check 1).
fn div(a: f32, b: f32) -> f32 {
    let q = opaque(a / b);
    return q + residual(a, q, b) / b;
}

// Seconds from the split time (hi, lo) to `now` (`split_seconds`).
fn seconds_since(hi: f32, lo: f32) -> f32 {
    return opaque(params.now_hi - hi) + opaque(params.now_lo - lo);
}

// The eased fraction `raw` of the way from one key to the next (`curve::ease`).
fn ease(code: u32, raw: f32) -> f32 {
    let t = clamp(raw, 0.0, 1.0);
    switch code {
        case EASE_STEP: {
            return 0.0;
        }
        case EASE_IN_QUAD: {
            return t * t;
        }
        case EASE_OUT_QUAD: {
            return 1.0 - (1.0 - t) * (1.0 - t);
        }
        case EASE_OUT_CUBIC: {
            let u = 1.0 - t;
            return 1.0 - u * u * u;
        }
        case EASE_IN_OUT: {
            return t * t * (3.0 - 2.0 * t);
        }
        default: {
            return t;
        }
    }
}

// Curve `id` at normalised time `t`, or `fallback` without that curve (`EffectCatalog::sample`
// and `Curve::sample`).
fn sample_curve(id: u32, t: f32, fallback: f32) -> f32 {
    if (id == NO_CURVE || id >= params.curves) {
        return fallback;
    }
    let range = curve_ranges[id];
    if (range.y == 0u) {
        return 0.0;
    }
    let first = curve_keys[range.x];
    if (t <= first.time) {
        return first.value;
    }
    for (var index = 0u; index + 1u < range.y; index += 1u) {
        let k0 = curve_keys[range.x + index];
        let k1 = curve_keys[range.x + index + 1u];
        if (t <= k1.time) {
            let fraction = ease(k0.easing, div(t - k0.time, k1.time - k0.time));
            return k0.value + (k1.value - k0.value) * fraction;
        }
    }
    return curve_keys[range.x + range.y - 1u].value;
}

// Whether a box of world length `size` at `point` is too small on screen (`CameraView::pixels`,
// `projected_px_with`, `lod_skips`).
fn below_detail(size: f32, point: vec3<f32>) -> bool {
    if (params.has_camera == 0u) {
        return false;
    }
    let offset = point - params.camera_position;
    let forward = params.camera_forward;
    let along = max(offset.x * forward.x + offset.y * forward.y + offset.z * forward.z, MIN_DISTANCE);
    let visible_height = 2.0 * along * params.half_tan;
    if (visible_height <= F32_EPSILON) {
        return false;
    }
    let px = div(size, visible_height) * params.viewport_px;
    return px < params.min_screen_px;
}

// The decal `record` draws now, or the zero decal (`age_decal`).
fn age_decal(record: DecalRecord) -> DecalInstance {
    let hidden = DecalInstance();
    let flags = record.flags;
    // decal_clock
    var detail_fade = 1.0;
    if ((flags & FLAG_DETAIL) != 0u && (flags & FLAG_DETAIL_RETIRED) != 0u) {
        detail_fade = clamp(
            1.0 - div(seconds_since(record.detail_retired_hi, record.detail_retired_lo), params.retire_secs),
            0.0,
            1.0,
        );
    }
    if (detail_fade <= 0.0) {
        return hidden;
    }
    var fade = detail_fade;
    if ((flags & FLAG_MARKS_RETIRED) != 0u) {
        fade = detail_fade * clamp(
            1.0 - div(seconds_since(record.marks_retired_hi, record.marks_retired_lo), params.mark_retire_secs),
            0.0,
            1.0,
        );
    }
    let since_birth = seconds_since(record.birth_hi, record.birth_lo) - record.start;
    var released_exit = 0.0;
    if ((flags & FLAG_RELEASED) != 0u) {
        released_exit = max(
            div(seconds_since(record.released_hi, record.released_lo), max(record.exit, MIN_EXIT)),
            0.0,
        );
    }
    let held = (flags & FLAG_HELD) != 0u;
    var exit_floor = 0.0;
    if (!(since_birth - 0.0 < 0.0 || !held) && released_exit < 1.0) {
        exit_floor = released_exit;
    }
    let age = since_birth - record.delay;
    if (age < 0.0) {
        return hidden;
    }
    var exit = 0.0;
    if (held) {
        if (released_exit >= 1.0) {
            return hidden;
        }
        exit = released_exit;
    } else if (age > record.span) {
        return hidden;
    }
    let life = clamp(div(age, record.span), 0.0, 1.0);
    // decal_at
    let intensity = sample_curve(record.curve, life, 1.0) * fade * (1.0 - exit * exit) * record.strength;
    let extent = record.size * sample_curve(record.size_curve, life, 1.0) * record.size_strength * record.jitter;
    if (below_detail(extent * 2.0, record.base.center)) {
        return hidden;
    }
    var out = record.base;
    if ((flags & FLAG_FLIPBOOK) != 0u) {
        // flipbook_frame(element, None, 0.0, life, true): fitted to the life, blended.
        let frames = max(record.flipbook_frames, 1u);
        let count = f32(frames);
        let position = clamp(life * count, 0.0, count - 1.0);
        let whole = u32(floor(position));
        out.layer_a = record.flipbook_first + whole;
        out.layer_b = record.flipbook_first + min(whole + 1u, frames - 1u);
        out.frame_mix = position - floor(position);
    }
    out.half_extents = vec3<f32>(extent, record.depth, extent * record.width);
    out.intensity = intensity;
    out.erosion = max(sample_curve(record.erosion_curve, life, 0.0), exit_floor);
    out.ramp_scale = sample_curve(record.ramp_curve, life, 1.0);
    return out;
}

@compute @workgroup_size(WORKGROUP)
fn apply_patches(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= params.patches) {
        return;
    }
    let entry = patches[index];
    rows[entry.row] = entry.record;
}

@compute @workgroup_size(WORKGROUP)
fn age(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= params.rows) {
        return;
    }
    decals[index] = age_decal(rows[order[index]]);
}
