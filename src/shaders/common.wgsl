// CPU simulation uploads display data only; physics and diagnostics stay f64.
struct Body {
    position: vec2<f32>,
    mass: f32,
    padding: f32,
}
struct State {
    bodies: array<Body, 3>,
    counters: vec4<u32>, // next history slot, history count, reserved, reserved
}
struct Params {
    camera: vec4<f32>, // center xy, half height, aspect ratio
    display: vec4<f32>, // viewport width/height, trail opacity, body size
    controls: vec4<u32>, // trail length, grid, sRGB target, reserved
}
const HISTORY: u32 = 4096u;
