// The minimap (plan docs/plans/2026-12-01-minimap.md D-1, D-3).
//
// One render pass into the minimap image, two pipelines:
// - `terrain_vs` / `terrain_fs`: one full-screen triangle. Each pixel maps to a map point through
//   the uniform's `offset` and `scale` (the CPU's `MinimapLayout`); pixels off the map are the
//   background. On the map, the ground is the splat-weighted sum of the theme's albedo layers,
//   each read at its last mip (the 1x1 linear average of the real baked texture), times the
//   brightness of the cell's cliff level; unwalkable cells are `unpathable`, and the high side of
//   a cliff edge is mixed toward the cliff layer's colour. Fog, sampled bilinearly so its edges
//   are soft, then lerps the colour toward the background, as 5.0.14's MinimapTerrain.fx lerps
//   to black.
// - `mark_vs` / `mark_fs`: one six-vertex quad per instance of the marks storage buffer, in
//   minimap pixels: a square filled with its colour inside a border `stroke` pixels wide, or a
//   segment `stroke` pixels wide. Fog never hides a mark: what is listed is what the viewer may
//   see. Instances past the buffer's `mark_count` are the camera frustum's four edges (D-8),
//   read from the uniform's corners, so a camera move needs no buffer write. Segments are cut at
//   the map rectangle, so a view past the map's edge is clipped there.
//
// Coordinates: `@builtin(position)` in the fragment stage is in target pixels from the top-left,
// pixel centres at +0.5. Map points are simulation (x, y) in world units, one unit per cell; the
// map's y = 0 row is the top of the image. Colours are linear; the target is sRGB-encoded.

struct MinimapUniform {
    background: vec4<f32>,
    unpathable: vec4<f32>,
    level_brightness: vec4<f32>,
    // Colour of a square's border.
    border: vec4<f32>,
    // Pixel of the map's (0, 0) corner.
    offset: vec2<f32>,
    // Map width and height in cells.
    map_size: vec2<f32>,
    // Pixels per world unit.
    scale: f32,
    // Side of the square target in pixels.
    side: f32,
    edge_mix: f32,
    // 1 draws the terrain, 0 leaves the map rectangle the background.
    terrain_on: u32,
    // Presented simulation tick.
    tick: u32,
    // Marks in the storage buffer; instances from here on are the frustum's edges.
    mark_count: u32,
    // 1 draws the frustum.
    frustum_on: u32,
    // Width of the frustum's edges in pixels.
    frustum_px: f32,
    frustum_color: vec4<f32>,
    // The view's ground trapezoid in pixels: top-left, top-right (xy, zw of [0]), bottom-right,
    // bottom-left (of [1]).
    frustum: array<vec4<f32>, 2>,
}

// One shape in minimap pixels (Rust `MinimapMark`, 48 bytes).
struct MinimapMark {
    a: vec2<f32>,
    b: vec2<f32>,
    color: vec4<f32>,
    kind: u32,
    start_tick: u32,
    stroke: f32,
    // A ring's duration in ticks; 0 otherwise.
    extra: f32,
}

@group(0) @binding(0) var<uniform> minimap: MinimapUniform;
// Rgba8Unorm, one texel per cell: R height, G walkable, B high cliff edge (minimap/terrain.rs).
@group(0) @binding(1) var cells: texture_2d<f32>;
@group(0) @binding(2) var splat: texture_2d<f32>;
@group(0) @binding(3) var splat_sampler: sampler;
@group(0) @binding(4) var albedo: texture_2d_array<f32>;
@group(0) @binding(5) var albedo_sampler: sampler;
@group(0) @binding(6) var<storage, read> marks: array<MinimapMark>;
// R8Unorm, one byte per cell: how far fog darkens the cell toward the background (D-4).
@group(0) @binding(7) var fog: texture_2d<f32>;

// Mark kinds (Rust `minimap::marks::MARK_*`).
const MARK_SQUARE: u32 = 0u;
const MARK_SEGMENT: u32 = 1u;
const MARK_RING: u32 = 2u;

// Albedo array layers, in `TerrainTheme::ordered_layers` order (terrain.wgsl).
const LAYER_GRASS: i32 = 0;
const LAYER_METAL: i32 = 1;
const LAYER_DIRT: i32 = 2;
const LAYER_CLIFF: i32 = 3;
// The highest cliff level; the cell texture's R is height / (3 levels).
const TOP_LEVEL: f32 = 3.0;

struct FullScreen {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn terrain_vs(@builtin(vertex_index) index: u32) -> FullScreen {
    // One triangle covering clip space: (-1, -1), (3, -1), (-1, 3).
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullScreen;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// The 1x1 average colour of albedo layer `layer`.
fn layer_average(layer: i32) -> vec3<f32> {
    let last = f32(textureNumLevels(albedo) - 1);
    return textureSampleLevel(albedo, albedo_sampler, vec2<f32>(0.5, 0.5), layer, last).rgb;
}

// Brightness of a cell whose normalized height is `height`, interpolated between levels.
fn level_brightness(height: f32) -> f32 {
    let level = clamp(height * TOP_LEVEL, 0.0, TOP_LEVEL);
    let low = u32(floor(level));
    let high = min(low + 1u, 3u);
    return mix(minimap.level_brightness[low], minimap.level_brightness[high], fract(level));
}

// The terrain colour of cell `cell`.
fn ground_color(cell: vec2<i32>) -> vec3<f32> {
    let texel = textureLoad(cells, cell, 0);
    if (texel.g < 0.5) {
        return minimap.unpathable.rgb;
    }
    let uv = (vec2<f32>(cell) + 0.5) / minimap.map_size;
    let weights = textureSampleLevel(splat, splat_sampler, uv, 0.0).rgb;
    let total = weights.r + weights.g + weights.b;
    var ground = layer_average(LAYER_DIRT);
    if (total > 1e-4) {
        ground = (layer_average(LAYER_GRASS) * weights.r
            + layer_average(LAYER_METAL) * weights.g
            + layer_average(LAYER_DIRT) * weights.b) / total;
    }
    let brightness = level_brightness(texel.r);
    var color = ground * brightness;
    if (texel.b > 0.5) {
        color = mix(color, layer_average(LAYER_CLIFF) * brightness, minimap.edge_mix);
    }
    return color;
}

@fragment
fn terrain_fs(in: FullScreen) -> @location(0) vec4<f32> {
    let world = (in.position.xy - minimap.offset) / minimap.scale;
    let on_map = all(world >= vec2<f32>(0.0)) && all(world < minimap.map_size);
    if (!on_map || minimap.terrain_on == 0u) {
        return vec4<f32>(minimap.background.rgb, 1.0);
    }
    let cell = vec2<i32>(floor(world));
    let darkening = textureSampleLevel(fog, splat_sampler, world / minimap.map_size, 0.0).r;
    return vec4<f32>(mix(ground_color(cell), minimap.background.rgb, darkening), 1.0);
}

struct MarkOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Pixel offset from the mark's centre.
    @location(1) local: vec2<f32>,
    // Half-size and border width, pixels.
    @location(2) @interpolate(flat) half_size: vec2<f32>,
    @location(3) @interpolate(flat) stroke: f32,
    @location(4) @interpolate(flat) kind: u32,
    // A ring's current radius (x) and opacity (y).
    @location(5) @interpolate(flat) ring: vec2<f32>,
}

// Corner `index` (0..4) of the frustum trapezoid, pixels.
fn frustum_corner(index: u32) -> vec2<f32> {
    let pair = minimap.frustum[index / 2u];
    if (index % 2u == 0u) {
        return pair.xy;
    }
    return pair.zw;
}

// The frustum's edge `edge` (0..4) as a segment mark.
fn frustum_edge(edge: u32) -> MinimapMark {
    var mark: MinimapMark;
    mark.a = frustum_corner(edge);
    mark.b = frustum_corner((edge + 1u) % 4u);
    mark.color = minimap.frustum_color;
    mark.kind = MARK_SEGMENT;
    mark.start_tick = 0u;
    mark.stroke = minimap.frustum_px;
    mark.extra = 0.0;
    return mark;
}

// Pixel of corner `vertex` of a segment's quad from `a` to `b`, `width` pixels wide, extended
// by half its width past each end so that joined segments close their corners.
fn segment_corner(vertex: u32, a: vec2<f32>, b: vec2<f32>, width: f32) -> vec2<f32> {
    let corner = quad_corner(vertex);
    let delta = b - a;
    let span = max(length(delta), 1e-4);
    let along = delta / span;
    let across = vec2<f32>(-along.y, along.x);
    let half = width * 0.5;
    let t = (corner.x * 0.5 + 0.5);
    return mix(a - along * half, b + along * half, t) + across * corner.y * half;
}

// Corner `vertex` of a mark's quad, two triangles, in units of the mark's half-size.
fn quad_corner(vertex: u32) -> vec2<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    return corners[vertex];
}

@vertex
fn mark_vs(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> MarkOut {
    var mark: MinimapMark;
    if (instance < minimap.mark_count) {
        mark = marks[instance];
    } else {
        mark = frustum_edge(instance - minimap.mark_count);
    }
    var pixel = mark.a + quad_corner(vertex) * mark.b;
    var ring = vec2<f32>(0.0, 1.0);
    if (mark.kind == MARK_SEGMENT) {
        pixel = segment_corner(vertex, mark.a, mark.b, mark.stroke);
    } else if (mark.kind == MARK_RING) {
        // The ring grows from b.x to b.y pixels and fades over `extra` presented ticks.
        let age = f32(max(minimap.tick, mark.start_tick) - mark.start_tick);
        let t = clamp(age / max(mark.extra, 1.0), 0.0, 1.0);
        ring = vec2<f32>(mix(mark.b.x, mark.b.y, t), 1.0 - t);
        pixel = mark.a + quad_corner(vertex) * (ring.x + mark.stroke);
    }
    let clip = vec2<f32>(pixel.x / minimap.side * 2.0 - 1.0, 1.0 - pixel.y / minimap.side * 2.0);
    var out: MarkOut;
    out.position = vec4<f32>(clip, 0.0, 1.0);
    out.color = mark.color;
    out.local = pixel - mark.a;
    out.half_size = mark.b;
    out.stroke = mark.stroke;
    out.kind = mark.kind;
    out.ring = ring;
    return out;
}

@fragment
fn mark_fs(in: MarkOut) -> @location(0) vec4<f32> {
    if (in.kind == MARK_SEGMENT) {
        // Segments are cut at the map rectangle.
        let world = (in.position.xy - minimap.offset) / minimap.scale;
        if (any(world < vec2<f32>(0.0)) || any(world >= minimap.map_size)) {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a);
    }
    if (in.kind == MARK_RING) {
        if (abs(length(in.local) - in.ring.x) > in.stroke * 0.5 || in.ring.y <= 0.0) {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * in.ring.y);
    }
    // Distance of this pixel's centre inside the square's nearest edge.
    let inside = in.half_size - abs(in.local);
    if (min(inside.x, inside.y) < in.stroke) {
        return vec4<f32>(minimap.border.rgb, 1.0);
    }
    return vec4<f32>(in.color.rgb, in.color.a);
}
