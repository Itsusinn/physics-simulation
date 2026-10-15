# Three Body

An interactive simulation of the classic Pythagorean three-body problem, with masses 3, 4, and 5. Built with Rust/WASM, Bevy 0.20, bevy_egui, WebGPU, and Trunk. Physics uses the Rust `ode_solvers` DOP853 integrator with CPU double precision; WebGPU renders bodies and trails.

[Live demo](https://itsusinn.github.io/apps/three-body/)

Adjust initial conditions and simulation parameters, explore orbital trails, and monitor energy and momentum. Requires a WebGPU-capable browser on HTTPS or localhost.

Adaptive integration uses relative tolerance `1e-11`, absolute tolerance `1e-13`, and maximum step `0.05` by default. Tolerances control estimated local error, not global trajectory accuracy. The classic initial conditions are paused on startup. Physics and conservation diagnostics remain `f64`; only display data are converted to `f32`.

## Run locally

With Rust 1.97.1+ installed (the repository pins 1.98.1):

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve --release
```

Open <http://127.0.0.1:8080/>. Use `trunk build --release --locked` to generate static files in `dist/`.

Bevy owns the window and render loop. A separate `Update` system advances DOP853 once per frame; egui layout and rendering only consume immutable display snapshots. Native desktop preview is also available with `cargo run`.

Release builds use WebGPU-only browser rendering and wasm-opt for size optimization. Trunk automatically downloads the pinned Binaryen 133 tool.

The measured Bevy migration bundle is 15,038,672 bytes of WASM (5,016,041 bytes estimated with gzip-9). CI enforces an 18 MB raw WASM budget. Compression estimates are not a promise of actual server transfer size.

## Scenes

The top **Scene** selector lists registered scenes. The current and default scene is `2d-345-threebody`; it is the only scene shipped so far. Selecting another scene takes effect on the next Bevy update, drops the previous scene's simulation, drafts and history, and creates the selected scene in its initial paused state. Selecting the current scene leaves its state intact. The selector stays visible when Parameters is hidden.

To add a scene, implement `Scene` in a new module under `src/scenes/` and add a unique stable ID and factory to `SCENES` in `src/scenes/mod.rs`. Each scene owns its state and provides `advance`, `ui`, and `is_running`. Factories should return a fresh paused scene; physics belongs in `advance`, while `ui` may run multiple times per frame. Scene implementations can use their own simulation and rendering code without changing the application shell. `src/scenes/three_body.rs` adapts the existing three-body implementation to this lifecycle.

See [AGENTS.md](AGENTS.md) for architecture, numerical limitations, testing, and deployment details.
