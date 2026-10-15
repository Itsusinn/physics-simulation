# physics-simulation

An interactive physics laboratory featuring the classic Pythagorean three-body problem, spatial three-body motion, a double pendulum, coupled spring oscillators, and Kepler orbits. Built with Rust/WASM, Bevy 0.20, bevy_egui, WebGPU, and Trunk. Physics uses runtime-selectable CPU double-precision integrators (DOP853 by default); WebGPU renders the experiments and trails.

[Live demo](https://itsusinn.github.io/apps/physics-simulation/)

Adjust initial conditions and simulation parameters, explore orbital trails, and monitor energy and momentum. Requires a WebGPU-capable browser on HTTPS or localhost.

Adaptive integration uses relative tolerance `1e-11`, absolute tolerance `1e-13`, and maximum step `0.05` by default. Tolerances control estimated local error, not global trajectory accuracy. The classic initial conditions are paused on startup. Physics and conservation diagnostics remain `f64`; only display data are converted to `f32`.

## Run locally

With Rust 1.97.1+ installed (the repository pins 1.98.1) and Python 3.9+ available as `python`:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve --release
```

Open <http://127.0.0.1:8080/>. Use `trunk build --release --locked` to generate static files in `dist/`.

Bevy owns the window and render loop. A separate `Update` system advances the selected integrator once per frame; egui layout and rendering only consume immutable display snapshots. Native desktop preview is also available with `cargo run`.

Release builds use WebGPU-only browser rendering and wasm-opt for size optimization. Trunk automatically downloads the pinned Binaryen 133 tool.

The startup overlay shows WASM download progress, then initialization status until the scene UI is ready. Loading errors offer a retry button. A Python standard-library post-build hook corrects Trunk 0.21.14's progress total to the final optimized WASM size, including when the server uses compression.

The measured Bevy migration bundle is 15,038,672 bytes of WASM (5,016,041 bytes estimated with gzip-9). CI enforces an 18 MB raw WASM budget. Compression estimates are not a promise of actual server transfer size.

## Algorithms and hot swapping

The top **Algorithm** selector lists algorithms compatible with the active scene and marks recommendations. **About algorithms** shows the method description, supported scenes, and the scene's recommended algorithms. Switching takes effect at the next physics update, preserving positions, velocities, simulation time, energy reference, initial-condition drafts, history and run/pause status. Only the solver's cached step guess is discarded. Reset and presets keep the selected algorithm; entering another scene selects that scene's first recommendation. Switching does not clear an existing numerical error; Reset is still required to recover.

| Algorithm ID | Method | Supported scenes |
| --- | --- | --- |
| `dop853` | Adaptive order 8, embedded 5/3 error estimate | All five |
| `dopri5` | Adaptive Dormand–Prince 5(4) | All five |
| `rk4` | Classical fourth-order Runge–Kutta | All five |
| `velocity-verlet` | Second-order kick–drift–kick | Both three-body scenes, springs, Kepler |

Three-body and double-pendulum scenes recommend DOP853 and DOPRI5. Springs and Kepler recommend DOP853 and Velocity Verlet. DOP853 remains the default everywhere. Verlet requires position-only forces and constant masses, so it is excluded from the current double-pendulum formulation, whose angular acceleration depends on velocity.

DOP853 and DOPRI5 use the existing [`ode_solvers` 0.6.2](https://docs.rs/ode_solvers/0.6.2/ode_solvers/) dependency. RK4 and Velocity Verlet are implemented in Rust without additional dependencies; Verlet follows the standard [kick–drift–kick formulation](https://manual.gromacs.org/documentation/2024-current/reference-manual/algorithms/molecular-dynamics.html#the-velocity-verlet-integrator). All methods keep f64 physics and diagnostics and share the 2048-attempt frame budget, last-valid-state protection, and no-catch-up policy.

RK4 and Verlet have no adaptive error control: tolerance controls are disabled, and **Max dt** sets the step, shortened at frame boundaries. Smaller steps and convergence checks are necessary, especially for close encounters. Verlet is symplectic for these separable models with fixed steps; the application does not guarantee a uniform step across rendering frames. Switching methods does not undo accumulated numerical error or guarantee long-term accuracy.

`src/integrators.rs` owns `ALGORITHMS`, each entry's `supported_scenes`, the backend dispatch, and bounded outputs. `src/scenes/mod.rs` owns each scene's `recommended_algorithms` and defers selection changes to Update. Adding a compiled-in method requires a stable algorithm ID, registry metadata and backend implementation; run `cargo test --jobs 2 integrators::` plus scene tests to verify compatibility, analytic accuracy, convergence and work limits. This is runtime switching among compiled-in methods, not downloading or dynamically loading executable plugins.

## Scenes

The top **Scene** selector lists five experiments: `2d-345-threebody` (the default), `3d-threebody`, `2d-double-pendulum`, `1d-coupled-springs`, and `2d-kepler-orbit`. Selecting another scene takes effect on the next Bevy update, drops the previous scene's simulation, drafts and history, and creates the selected scene in its initial paused state. Selecting the current scene leaves its state intact. The selector stays visible when Parameters is hidden.

`3d-threebody` integrates all 18 position/velocity components with CPU f64 integration (DOP853 by default). Its default is a spatial perturbation of the 3-4-5 system with zero center of mass and total momentum. Edit mass, x/y/z position and velocity, G, and softening under Parameters, then Apply & restart. **Classic 3-4-5 in XY plane** loads the original initial conditions into 3D; with zero z position/velocity, motion stays planar.

Drag to orbit, Shift-drag or right-drag to pan, scroll to zoom, and double-click or use **Fit all bodies** to frame the system. The XY grid includes red X, green Y, and blue Z axes. **Reset camera** restores the viewing direction; **Auto frame** follows all bodies. Sphere sizes are visual markers, not collision radii. Perspective spheres, spatial trails and grid use the existing WebGPU callback path with an offscreen depth buffer, without new dependencies or Bevy PBR. The offscreen target is capped at 4096 pixels per side while preserving aspect ratio. Diagnostics show energy and all three components of linear/angular momentum. Histories keep up to 4096 samples and reset when switching scenes or restarting.

### Classic mechanics experiments

| Scene | Model and presets | Diagnostics |
| --- | --- | --- |
| `2d-double-pendulum` | Two point masses on massless rigid rods, uniform gravity; large-angle and small-angle presets | Kinetic/potential energy, scaled energy drift, both bob trails |
| `1d-coupled-springs` | Two masses between equal outer springs, joined by a coupling spring; energy exchange, in-phase and opposite-phase presets | Individual oscillator and coupling energies, displacement versus time |
| `2d-kepler-orbit` | Test particle in a fixed inverse-square field; elliptical, circular and escape presets | Specific energy, specific angular momentum, eccentricity and orbit trail |

These scenes use the shared selectable f64 integrators and bevy_egui WebGPU renderer, with no new dependencies. **Run / Pause**, **Step**, **Reset**, Space and R work in every experiment. Edit masses, lengths, spring constants, gravity or initial positions/velocities in **Parameters**, then **Apply & restart**. Reset restores the last applied configuration; preset buttons apply and restart immediately on the next Update. All experiments restart paused. Angles use radians; other quantities use consistent arbitrary units. There is no damping or collision response. Kepler is a fixed-center test-particle model, not two mutually accelerated bodies.

Drag to pan, scroll to zoom, double-click or **Fit experiment** to fit. **Auto frame** follows the experiment and visible history. Spring drawings are schematic; the chart shows actual displacements and simulation time. Histories contain at most 4096 samples, sampled once per advancing frame. Parameters can be hidden on narrow screens without hiding run controls or scene selection.

The new scenes use the same default tolerances, maximum step and 2048-attempt budget as the three-body scenes. Wall-clock time is capped at 0.05 seconds per frame before applying Speed; budget exhaustion drops uncomputed time. Invalid states stop the simulation and preserve the last valid accepted state. Kepler integration stops at a numerical exclusion radius of `1e-6` (not a collision radius or softening). Scaled energy drift is `(E-E0)/max(|K0|+|U0|, 1e-15)`, which remains usable for near-zero total orbital energy. Double-pendulum potential energy is zero at the hanging equilibrium. Conservation is a diagnostic, not a guarantee of trajectory accuracy.

Tests compare spring normal modes and Kepler periods against analytic results, double-pendulum motion against an independent RK4 implementation of the [explicit equations of motion](https://www.myphysicslab.com/pendulum/double-pendulum-en.html), and check energy conservation, singularity handling, work limits, reset, histories, switching and multi-pass narrow/wide UI tessellation. Run the focused suite with `cargo test --jobs 2 scenes::mechanics:: -- --nocapture`.

To add a scene, implement `Scene` in a new module under `src/scenes/` and add a unique stable ID and factory to `SCENES` in `src/scenes/mod.rs`. Each scene owns its state and provides `advance`, `ui`, `is_running`, `algorithm`, and compatibility-checked `set_algorithm`. Declare its ordered `recommended_algorithms` (first entry is the default) and include its ID in each compatible algorithm's `supported_scenes`. Factories should return a fresh paused scene; physics belongs in `advance`, while `ui` may run multiple times per frame. Scene implementations can use their own simulation and rendering code without changing the application shell. `src/scenes/three_body.rs` adapts the existing three-body implementation to this lifecycle. `src/scenes/mechanics/` shares controls and bounded integration across the three classic mechanics models.

See [AGENTS.md](AGENTS.md) for architecture, numerical limitations, testing, and deployment details.
