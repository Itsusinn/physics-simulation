# physics-simulation

An interactive physics laboratory featuring the classic Pythagorean three-body problem, spatial three-body motion, a double pendulum, coupled spring oscillators, Kepler orbits, and colliding sliding blocks. Built with Rust/WASM, Bevy 0.20, bevy_egui, WebGPU, and Trunk. Smooth mechanics uses runtime-selectable CPU double-precision integrators (DOP853 by default); sliding blocks use Avian collision physics; WebGPU renders the experiments and trails.

[Live demo](https://itsusinn.github.io/apps/physics-simulation/)

Adjust initial conditions and simulation parameters, explore orbital trails, and monitor energy and momentum. Requires a WebGPU-capable browser on HTTPS or localhost.

Adaptive integration uses relative tolerance `1e-11`, absolute tolerance `1e-13`, and maximum step `0.05` by default. Tolerances control estimated local error, not global trajectory accuracy. The classic initial conditions are paused on startup. For the ODE experiments, physics and conservation diagnostics remain `f64`; only display data are converted to `f32`. The Avian scene has its own fixed-step collision solver, described below.

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

The measured Bevy migration bundle is 15,038,672 bytes of WASM (5,016,041 bytes estimated with gzip-9). The Avian sliding-block build measures 18,638,724 bytes raw and 5,965,105 bytes estimated with gzip-9 on Windows (2026-10-15). CI enforces the user-approved 20 MB raw WASM budget. Compression estimates are not a promise of actual server transfer size.

## Algorithms and hot swapping

The top **Algorithm** selector lists algorithms compatible with the active scene and marks recommendations. **About algorithms** shows the method description, supported scenes, and the scene's recommended algorithms. Switching takes effect at the next physics update, preserving positions, velocities, simulation time, energy reference, initial-condition drafts, history and run/pause status. Only the solver's cached step guess is discarded. Reset and presets keep the selected algorithm; entering another scene selects that scene's first recommendation. Switching does not clear an existing numerical error; Reset is still required to recover.

| Algorithm ID | Method | Supported scenes |
| --- | --- | --- |
| `dop853` | Adaptive order 8, embedded 5/3 error estimate | All five ODE scenes |
| `dopri5` | Adaptive Dormand–Prince 5(4) | All five ODE scenes |
| `rk4` | Classical fourth-order Runge–Kutta | All five ODE scenes |
| `velocity-verlet` | Second-order kick–drift–kick | Both three-body scenes, springs, Kepler |

Three-body and double-pendulum scenes recommend DOP853 and DOPRI5. Springs and Kepler recommend DOP853 and Velocity Verlet. DOP853 remains the default for the five ODE scenes. Verlet requires position-only forces and constant masses, so it is excluded from the current double-pendulum formulation, whose angular acceleration depends on velocity.

DOP853 and DOPRI5 use the existing [`ode_solvers` 0.6.2](https://docs.rs/ode_solvers/0.6.2/ode_solvers/) dependency. RK4 and Velocity Verlet are implemented in Rust without additional dependencies; Verlet follows the standard [kick–drift–kick formulation](https://manual.gromacs.org/documentation/2024-current/reference-manual/algorithms/molecular-dynamics.html#the-velocity-verlet-integrator). All methods keep f64 physics and diagnostics and share the 2048-attempt frame budget, last-valid-state protection, and no-catch-up policy.

RK4 and Verlet have no adaptive error control: tolerance controls are disabled, and **Max dt** sets the step, shortened at frame boundaries. Smaller steps and convergence checks are necessary, especially for close encounters. Verlet is symplectic for these separable models with fixed steps; the application does not guarantee a uniform step across rendering frames. Switching methods does not undo accumulated numerical error or guarantee long-term accuracy.

`src/integrators.rs` owns `ALGORITHMS`, each entry's `supported_scenes`, the backend dispatch, and bounded outputs. `src/scenes/mod.rs` owns each scene's `recommended_algorithms` and defers selection changes to Update. Adding a compiled-in method requires a stable algorithm ID, registry metadata and backend implementation; run `cargo test --jobs 2 integrators::` plus scene tests to verify compatibility, analytic accuracy, convergence and work limits. This is runtime switching among compiled-in methods, not downloading or dynamically loading executable plugins.

## Scenes

The top **Scene** selector lists six experiments: `2d-345-threebody` (the default), `3d-threebody`, `2d-double-pendulum`, `1d-coupled-springs`, `2d-kepler-orbit`, and `2d-block-collisions`. Selecting another scene takes effect on the next Bevy update, drops the previous scene's simulation, drafts and history, and creates the selected scene in its initial paused state. Selecting the current scene leaves its state intact. The selector stays visible when Parameters is hidden.

`3d-threebody` integrates all 18 position/velocity components with CPU f64 integration (DOP853 by default). Its default is a spatial perturbation of the 3-4-5 system with zero center of mass and total momentum. Edit mass, x/y/z position and velocity, G, and softening under Parameters, then Apply & restart. **Classic 3-4-5 in XY plane** loads the original initial conditions into 3D; with zero z position/velocity, motion stays planar.

Drag to orbit, Shift-drag or right-drag to pan, scroll to zoom, and double-click or use **Fit all bodies** to frame the system. The XY grid includes red X, green Y, and blue Z axes. **Reset camera** restores the viewing direction; **Auto frame** follows all bodies. Sphere sizes are visual markers, not collision radii. Perspective spheres, spatial trails and grid use the existing WebGPU callback path with an offscreen depth buffer, without new dependencies or Bevy PBR. The offscreen target is capped at 4096 pixels per side while preserving aspect ratio. Diagnostics show energy and all three components of linear/angular momentum. Histories keep up to 4096 samples and reset when switching scenes or restarting.

### Sliding blocks and an L-shaped wall

`2d-block-collisions` uses [Avian PR #1070](https://github.com/avianphysics/avian/pull/1070), pinned to `34c9b008d2f40638a953d9218db48d54f661365d`. This port shares Bevy 0.20 with the application; its three Git dependencies are also pinned by the upstream manifests and Cargo.lock. Only the required contact plugins are registered; spatial queries, joints and interpolation are omitted. Default Avian features are disabled; only 2D, f64 and the Parry f64 collider backend are enabled.

The L-shaped boundary consists of a left wall and horizontal rail. The small block (mass 1, side 0.7, center x = 2) starts at rest; the large block (mass 100, side 1.4, center x = 5) moves left at speed 1. The blocks collide with each other and the small block rebounds from the wall. Vertical motion and rotation are locked, gravity and friction are zero. Shapes match the collider dimensions. The rail is drawn across the view; horizontal motion is constrained even beyond its finite collider.

Parameters offers independent positive masses for both blocks, including decimals and values above 100, incoming speed 0.1–3 and restitution 0–1. Double-click either mass to type a value. Mass and inverse mass must remain normal finite f32 values in the collision solver. Defaults use elastic restitution 1; smaller values dissipate energy. Edits apply with **Apply & restart**; Reset uses the last applied settings. The scene starts paused and supports Run/Pause, Step (0.05 simulated seconds), Space, R, pan, zoom and Auto frame. A prominent total collision counter stays visible in the experiment when Parameters is hidden, with block/block and small/wall subtotals. The total is their sum and returns to zero on Reset or Apply & restart. Diagnostics also show velocities, kinetic energy and relative kinetic-energy change. Persistent contact is counted once, not once per solver step.

Avian owns a scene-local headless Bevy World, using a fixed 0.001 s step with eight solver substeps from `advance` only. Swept CCD covers all body types; speculative contacts are disabled and overlap correction is limited to 0.1 units/s to reduce artificial rebound in inelastic stacks. The frame admits at most 0.05 wall-clock seconds before Speed and performs at most 256 physics steps; excess whole steps are dropped, with a visible budget message. Fractional steps accumulate. Rendering never advances physics. Switching away drops this world and its contacts. The algorithm selector displays Avian for this scene; ODE methods are unavailable because they do not resolve contacts.

This Avian revision uses f64 positions, with f32 velocities, mass, collider dimensions and contact solving. Diagnostics convert the engine state to f64; this does not recover precision lost inside the solver. Input masses are rounded to f32 for the solver; kinetic-energy diagnostics use those same rounded masses. Mass rounding, inverse-mass rounding and the contact solver introduce numerical error, especially at extreme mass ratios. Elastic restitution is not a promise of exact energy conservation or mathematically exact collision counts. Non-finite states, excessive overlap or out-of-range display data stop the simulation while retaining the last valid snapshot; Reset recovers.

`src/scenes/blocks/` contains the physics world, controls, rendering and tests. Focused checks: `cargo test --jobs 2 scenes::blocks:: -- --nocapture`.

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

To add a scene, implement `Scene` in a new module under `src/scenes/` and add a unique stable ID and factory to `SCENES` in `src/scenes/mod.rs`. Each scene owns its state and provides `advance`, `ui`, `is_running`, `algorithm`, and compatibility-checked `set_algorithm`. Declare its ordered `recommended_algorithms` (first entry is the default; dedicated collision scenes return `None` from `algorithm` and use an empty ODE recommendation list plus `fixed_solver` metadata with supported scenes) and include its ID in each compatible algorithm's `supported_scenes`. Factories should return a fresh paused scene; physics belongs in `advance`, while `ui` may run multiple times per frame. Scene implementations can use their own simulation and rendering code without changing the application shell. `src/scenes/three_body.rs` adapts the existing three-body implementation to this lifecycle. `src/scenes/mechanics/` shares controls and bounded integration across the three classic mechanics models.

See [AGENTS.md](AGENTS.md) for architecture, numerical limitations, testing, and deployment details.
