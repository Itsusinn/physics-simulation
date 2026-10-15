struct Body {
    position: vec2<f32>,
    velocity: vec2<f32>,
    mass: f32,
    position_error_x: f32,
    position_error_y: f32,
    pad2: f32,
}
struct State {
    bodies: array<Body, 3>,
    metrics: vec4<f32>, // time, initial energy, energy, minimum separation
    diagnostics: vec4<f32>, // momentum xy, angular momentum, smallest step
    counters: vec4<u32>, // next history slot, history count, last substeps, status bits
}
struct Params {
    physics: vec4<f32>, // G, Plummer epsilon, maximum dt, frame duration
    camera: vec4<f32>, // center xy, half height, aspect ratio
    display: vec4<f32>, // viewport width/height, trail opacity, body size
    controls: vec4<u32>, // trail length, grid, adaptive timestep, reserved
}
const HISTORY: u32 = 4096u;
