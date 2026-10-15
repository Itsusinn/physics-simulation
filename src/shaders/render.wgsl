@group(0) @binding(0) var<storage, read> state: State;
@group(0) @binding(1) var<storage, read> history: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
// Palette values were authored in sRGB. Bevy's sRGB attachment performs the
// final encoding, so feed it linear RGB to avoid brightening the whole scene.
fn output_color(color: vec4<f32>) -> vec4<f32> {
    if (params.controls.z == 0u) { return color; }
    let linear = select(color.rgb / 12.92,
        pow((color.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        color.rgb > vec3<f32>(0.04045));
    return vec4<f32>(linear, color.a);
}
fn palette(i: u32) -> vec3<f32> {
    return array<vec3<f32>, 3>(vec3<f32>(0.30, 0.77, 1.0), vec3<f32>(1.0, 0.65, 0.27), vec3<f32>(0.78, 0.57, 1.0))[i];
}
fn project(p: vec2<f32>) -> vec2<f32> {
    return (p - params.camera.xy) / vec2<f32>(params.camera.z * params.camera.w, params.camera.z);
}
fn corner(i: u32) -> vec2<f32> {
    return array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0))[i];
}
@vertex
fn background_vs(@builtin(vertex_index) index: u32) -> Vertex {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0))[index];
    return Vertex(vec4<f32>(p, 0.0, 1.0), p, vec4<f32>(1.0));
}
@fragment
fn background_fs(v: Vertex) -> @location(0) vec4<f32> {
    let world = v.uv * vec2<f32>(params.camera.z * params.camera.w, params.camera.z) + params.camera.xy;
    var color = mix(vec3<f32>(0.022, 0.035, 0.060), vec3<f32>(0.038, 0.060, 0.095), max(0.0, 1.0 - length(v.uv) * 0.6));
    let desired = max(params.camera.z / 4.0, 0.0001);
    var spacing = pow(10.0, floor(log2(desired) / log2(10.0)));
    let ratio = desired / spacing;
    if (ratio > 5.0) { spacing *= 10.0; }
    else if (ratio > 2.0) { spacing *= 5.0; }
    else if (ratio > 1.0) { spacing *= 2.0; }
    let grid_p = world / spacing;
    let cell_distance = abs(fract(grid_p - 0.5) - 0.5) / max(fwidth(grid_p), vec2<f32>(0.0001));
    let grid = 1.0 - min(min(cell_distance.x, cell_distance.y), 1.0);
    let axis_distance = abs(world) / max(fwidth(world), vec2<f32>(0.0001));
    let axis = 1.0 - min(min(axis_distance.x, axis_distance.y), 1.0);
    if (params.controls.y != 0u) {
        color += vec3<f32>(0.035, 0.055, 0.08) * grid + vec3<f32>(0.04, 0.07, 0.09) * axis;
    }
    return output_color(vec4<f32>(color, 1.0));
}
@vertex
fn body_vs(@builtin(vertex_index) index: u32, @builtin(instance_index) body: u32) -> Vertex {
    let uv = corner(index);
    let radius = (15.0 + sqrt(state.bodies[body].mass) * 3.0) * params.display.w;
    let p = project(state.bodies[body].position) + uv * radius * 2.0 / params.display.xy;
    return Vertex(vec4<f32>(p, 0.0, 1.0), uv, vec4<f32>(palette(body), 1.0));
}
@fragment
fn body_fs(v: Vertex) -> @location(0) vec4<f32> {
    let r = length(v.uv);
    let core = 1.0 - smoothstep(0.19, 0.27, r);
    let halo = exp(-r * r * 7.0) * 0.55;
    let ring = (1.0 - smoothstep(0.012, 0.03, abs(r - 0.44))) * 0.25;
    return output_color(vec4<f32>(mix(v.color.rgb, vec3<f32>(0.96), core * 0.75), max(core, halo + ring) * (1.0 - smoothstep(0.8, 1.0, r))));
}
@vertex
fn trail_vs(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let body = instance / (HISTORY - 1u);
    let age = instance % (HISTORY - 1u);
    let count = min(state.counters.y, params.controls.x);
    if (age + 1u >= count) {
        return Vertex(vec4<f32>(2.0, 2.0, 0.0, 1.0), vec2<f32>(0.0), vec4<f32>(0.0));
    }
    let a_index = (state.counters.x + HISTORY - 1u - age) % HISTORY;
    let b_index = (a_index + HISTORY - 1u) % HISTORY;
    let a = project(history[body * HISTORY + a_index].xy);
    let b = project(history[body * HISTORY + b_index].xy);
    let delta = (b - a) * params.display.xy;
    let normal = vec2<f32>(-delta.y, delta.x) / max(length(delta), 0.00001);
    let uv = corner(index);
    let p = mix(a, b, (uv.x + 1.0) * 0.5) + normal * uv.y * 1.6 / params.display.xy;
    let alpha = pow(1.0 - f32(age) / f32(max(count, 1u)), 1.3) * params.display.z;
    return Vertex(vec4<f32>(p, 0.0, 1.0), uv, vec4<f32>(palette(body), alpha));
}
@fragment
fn trail_fs(v: Vertex) -> @location(0) vec4<f32> {
    return output_color(vec4<f32>(v.color.rgb, v.color.a * (1.0 - smoothstep(0.35, 1.0, abs(v.uv.y)))));
}
