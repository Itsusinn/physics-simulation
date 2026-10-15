# Three Body

An interactive simulation of the classic Pythagorean three-body problem, with masses 3, 4, and 5. Built with Rust/WASM, WebGPU, egui, and Trunk. Physics uses the Rust `ode_solvers` DOP853 integrator with CPU double precision; WebGPU renders bodies and trails.

[Live demo](https://itsusinn.github.io/apps/three-body/)

Adjust initial conditions and simulation parameters, explore orbital trails, and monitor energy and momentum. Requires a WebGPU-capable browser on HTTPS or localhost.

Adaptive integration uses relative tolerance `1e-11`, absolute tolerance `1e-13`, and maximum step `0.05` by default. Tolerances control estimated local error, not global trajectory accuracy. The classic initial conditions are paused on startup. Physics and conservation diagnostics remain `f64`; only display data are converted to `f32`.

## Run locally

With Rust installed:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve --release
```

Open <http://127.0.0.1:8080/>. Use `trunk build --release --locked` to generate static files in `dist/`.

Release builds use WebGPU-only dependencies and wasm-opt for size optimization. Trunk automatically downloads the pinned Binaryen 133 tool.

See [AGENTS.md](AGENTS.md) for architecture, numerical limitations, testing, and deployment details.
