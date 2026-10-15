struct Uniforms {
    transform: mat4x4<f32>,
    eye: vec4<f32>, // xyz, display radius
    focus: vec4<f32>, // xyz, grid spacing
    view: vec4<f32>, // viewport xy, trail opacity, body size
    options: vec4<u32>, // sRGB target, grid, trail count, reserved
}
struct Segment { start: vec4<f32>, end: vec4<f32>, color: vec4<f32> }
@group(0) @binding(0) var<uniform> params: Uniforms;
@group(0) @binding(1) var<storage, read> bodies: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> segments: array<Segment>;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) normal: vec3<f32>,
}
fn palette(i: u32) -> vec3<f32> {
    return array<vec3<f32>, 3>(vec3<f32>(0.30, 0.77, 1.0), vec3<f32>(1.0, 0.65, 0.27), vec3<f32>(0.78, 0.57, 1.0))[i];
}
fn output_color(color: vec4<f32>) -> vec4<f32> {
    if (params.options.x == 0u) { return color; }
    return vec4<f32>(select(color.rgb / 12.92,
        pow((color.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        color.rgb > vec3<f32>(0.04045)), color.a);
}
@vertex
fn sphere_vs(@builtin(vertex_index) index: u32, @builtin(instance_index) body: u32) -> Vertex {
    let corners = array<vec2<u32>, 6>(vec2<u32>(0,0), vec2<u32>(1,0), vec2<u32>(0,1), vec2<u32>(0,1), vec2<u32>(1,0), vec2<u32>(1,1));
    let cell = index / 6u;
    let corner = corners[index % 6u];
    let latitude = f32(cell / 24u + corner.y) / 16.0 * 3.14159265;
    let longitude = f32(cell % 24u + corner.x) / 24.0 * 6.2831853;
    let normal = vec3<f32>(sin(latitude) * cos(longitude), sin(latitude) * sin(longitude), cos(latitude));
    let radius = params.eye.w * params.view.w * pow(bodies[body].w / 4.0, 0.3333333);
    let position = bodies[body].xyz + radius * normal;
    return Vertex(params.transform * vec4<f32>(position, 1.0), vec4<f32>(palette(body), 1.0), normal);
}
@fragment
fn sphere_fs(v: Vertex) -> @location(0) vec4<f32> {
    let n = normalize(v.normal);
    let light = normalize(vec3<f32>(-0.4, 0.5, 0.8));
    let shade = 0.28 + 0.72 * max(dot(n, light), 0.0);
    let highlight = pow(max(dot(n, normalize(light + vec3<f32>(0.2, -0.4, 0.8))), 0.0), 32.0);
    return output_color(vec4<f32>(v.color.rgb * shade + vec3<f32>(highlight * 0.3), 1.0));
}
fn ribbon(start: vec3<f32>, end: vec3<f32>, color: vec4<f32>, index: u32) -> Vertex {
    var a = params.transform * vec4<f32>(start, 1.0);
    var b = params.transform * vec4<f32>(end, 1.0);
    if (a.z <= 0.0 && b.z <= 0.0) {
        return Vertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), vec4<f32>(0.0), vec3<f32>(0.0));
    }
    // Clip against the near plane before the perspective divide.
    if (a.z < 0.0) { a = mix(a, b, -a.z / (b.z - a.z)); }
    if (b.z < 0.0) { b = mix(b, a, -b.z / (a.z - b.z)); }
    let delta = (b.xy / b.w - a.xy / a.w) * params.view.xy;
    let side = vec2<f32>(-delta.y, delta.x) / max(length(delta), 0.00001);
    let corners = array<vec2<f32>, 6>(vec2<f32>(0,-1), vec2<f32>(1,-1), vec2<f32>(0,1), vec2<f32>(0,1), vec2<f32>(1,-1), vec2<f32>(1,1));
    let corner = corners[index];
    var p = mix(a, b, corner.x);
    p = vec4<f32>(p.xy + side * corner.y * 1.25 / params.view.xy * p.w, p.zw);
    return Vertex(p, color, vec3<f32>(0.0));
}
@vertex
fn trail_vs(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let segment = segments[instance];
    return ribbon(segment.start.xyz, segment.end.xyz, vec4<f32>(segment.color.rgb, segment.color.a * params.view.z), index);
}
@vertex
fn grid_vs(@builtin(vertex_index) index: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let spacing = params.focus.w;
    let center = floor(params.focus.xy / spacing) * spacing;
    let offset = (f32(instance % 21u) - 10.0) * spacing;
    var a = vec3<f32>(center.x + offset, center.y - 10.0 * spacing, 0.0);
    var b = vec3<f32>(center.x + offset, center.y + 10.0 * spacing, 0.0);
    var color = vec4<f32>(0.19, 0.27, 0.36, 0.4);
    if (instance >= 21u) {
        a = vec3<f32>(center.x - 10.0 * spacing, center.y + offset, 0.0);
        b = vec3<f32>(center.x + 10.0 * spacing, center.y + offset, 0.0);
    }
    if (instance >= 42u) {
        a = vec3<f32>(0.0);
        b = vec3<f32>(0.0);
        b[instance - 42u] = spacing * 4.0;
        color = array<vec4<f32>, 3>(vec4<f32>(0.95,0.35,0.35,0.9), vec4<f32>(0.35,0.9,0.5,0.9), vec4<f32>(0.35,0.55,1.0,0.9))[instance - 42u];
    }
    return ribbon(a, b, color, index);
}
@fragment
fn line_fs(v: Vertex) -> @location(0) vec4<f32> { return output_color(v.color); }

@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;
struct BlitVertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex
fn blit_vs(@builtin(vertex_index) index: u32) -> BlitVertex {
    let uv = array<vec2<f32>, 3>(vec2<f32>(0,0), vec2<f32>(2,0), vec2<f32>(0,2))[index];
    return BlitVertex(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}
@fragment
fn blit_fs(v: BlitVertex) -> @location(0) vec4<f32> { return textureSample(image, image_sampler, v.uv); }
