//! Runtime-selectable f64 integrators. Models own state/diagnostics; backends
//! return bounded accepted samples, never mutate a scene or its history.
use ode_solvers::{Dop853, Dopri5, OutputType, SVector, System, dop_shared::IntegrationError};

use crate::model::FrameParams;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Algorithm {
    #[default]
    Dop853,
    Dopri5,
    Rk4,
    VelocityVerlet,
}

pub struct AlgorithmDefinition {
    pub algorithm: Algorithm,
    pub id: &'static str,
    pub name: &'static str,
    pub supported_scenes: &'static [&'static str],
    pub adaptive: bool,
    pub description: &'static str,
}

const ALL_SCENES: &[&str] = &[
    "2d-345-threebody",
    "3d-threebody",
    "2d-double-pendulum",
    "1d-coupled-springs",
    "2d-kepler-orbit",
];

pub const ALGORITHMS: &[AlgorithmDefinition] = &[
    AlgorithmDefinition {
        algorithm: Algorithm::Dop853,
        id: "dop853",
        name: "DOP853",
        supported_scenes: ALL_SCENES,
        adaptive: true,
        description: "Order 8, adaptive 5/3 error estimate. High-accuracy general-purpose integration.",
    },
    AlgorithmDefinition {
        algorithm: Algorithm::Dopri5,
        id: "dopri5",
        name: "Dormand-Prince 5(4)",
        supported_scenes: ALL_SCENES,
        adaptive: true,
        description: "Order 5, adaptive order-4 error estimate. General-purpose integration with fewer stages per step.",
    },
    AlgorithmDefinition {
        algorithm: Algorithm::Rk4,
        id: "rk4",
        name: "Classical RK4",
        supported_scenes: ALL_SCENES,
        adaptive: false,
        description: "Order 4, fixed maximum step. No error control; reduce Max dt and check convergence.",
    },
    AlgorithmDefinition {
        algorithm: Algorithm::VelocityVerlet,
        id: "velocity-verlet",
        name: "Velocity Verlet",
        supported_scenes: &[
            "2d-345-threebody",
            "3d-threebody",
            "1d-coupled-springs",
            "2d-kepler-orbit",
        ],
        adaptive: false,
        description: "Order 2, kick-drift-kick. For position-only forces with constant masses. Fixed steps preserve symplectic structure; frame boundaries may shorten steps. No error control; close encounters need small Max dt.",
    },
];

impl Algorithm {
    pub fn definition(self) -> &'static AlgorithmDefinition {
        ALGORITHMS
            .iter()
            .find(|entry| entry.algorithm == self)
            .expect("registered algorithm")
    }

    pub fn supports(self, scene: &str) -> bool {
        self.definition().supported_scenes.contains(&scene)
    }
}

pub struct Request<'a> {
    pub params: &'a FrameParams,
    pub start: f64,
    pub duration: f64,
    pub next_step: f64,
    pub budget: u32,
    /// Each body is stored as [q_1..q_d, v_1..v_d]. Only supplied for
    /// separable, position-force models; the double pendulum supplies None.
    pub position_dimensions: Option<usize>,
}

pub struct Output<const N: usize> {
    pub samples: Vec<(f64, SVector<f64, N>)>,
    pub result: Result<(), IntegrationError>,
}

pub fn integrate<const N: usize>(
    system: impl System<f64, SVector<f64, N>>,
    state: SVector<f64, N>,
    request: Request<'_>,
) -> Output<N> {
    let p = request.params;
    let end = request.start + request.duration;
    if request.budget == 0 {
        return Output {
            samples: Vec::new(),
            result: Err(IntegrationError::MaxNumStepReached {
                x: request.start,
                n_step: 0,
            }),
        };
    }
    // Keep the established DOP853 configuration and Sparse accepted outputs.
    // Dormand-Prince uses its standard controller parameters (beta=.04).
    macro_rules! adaptive {
        ($solver:ident, $beta:expr, $min:expr, $max:expr, $budget:expr) => {{
            let mut solver = $solver::from_param(
                system,
                request.start,
                end,
                request.duration,
                state,
                p.rtol,
                p.atol,
                0.9,
                $beta,
                $min,
                $max,
                p.max_step.min(request.duration),
                request.next_step.min(p.max_step).min(request.duration),
                $budget,
                1000,
                OutputType::Sparse,
            );
            let result = solver.integrate().map(|_| ());
            Output {
                samples: solver
                    .x_out()
                    .iter()
                    .copied()
                    .zip(solver.y_out().iter().copied())
                    .skip(1)
                    .collect(),
                result,
            }
        }};
    }
    match p.algorithm {
        Algorithm::Dop853 => adaptive!(Dop853, 0.0, 0.333, 6.0, request.budget),
        // ode_solvers 0.6.2 Dopri5 checks n_step > n_max before incrementing;
        // Dop853 checks >=. Translate the budget to enforce the same limit.
        Algorithm::Dopri5 => adaptive!(Dopri5, 0.04, 0.2, 10.0, request.budget - 1),
        Algorithm::Rk4 | Algorithm::VelocityVerlet => fixed(system, state, request),
    }
}

fn fixed<const N: usize>(
    mut system: impl System<f64, SVector<f64, N>>,
    mut state: SVector<f64, N>,
    request: Request<'_>,
) -> Output<N> {
    let mut output = Output {
        samples: Vec::new(),
        result: Ok(()),
    };
    let mut time = request.start;
    let end = time + request.duration;
    for _ in 0..request.budget {
        if time >= end {
            return output;
        }
        let next_time = (time + request.params.max_step).min(end);
        let h = next_time - time;
        if !h.is_finite() || h <= 0.0 {
            output.result = Err(IntegrationError::StepSizeUnderflow { x: time });
            return output;
        }
        let mut k1 = SVector::<f64, N>::zeros();
        system.system(time, &state, &mut k1);
        let next = match request.params.algorithm {
            Algorithm::Rk4 => {
                // Classical fourth-order Runge-Kutta (not an adaptive method).
                let mut k2 = k1;
                let mut k3 = k1;
                let mut k4 = k1;
                system.system(time + h * 0.5, &(state + k1 * (h * 0.5)), &mut k2);
                system.system(time + h * 0.5, &(state + k2 * (h * 0.5)), &mut k3);
                system.system(next_time, &(state + k3 * h), &mut k4);
                state + (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (h / 6.0)
            }
            Algorithm::VelocityVerlet => {
                // Half kick, full drift, half kick. Full-time velocities make
                // switching to/from a Runge-Kutta method state preserving.
                let d = request
                    .position_dimensions
                    .expect("Verlet requires a separable model");
                assert!(d > 0 && N.is_multiple_of(2 * d));
                let mut next = state;
                for base in (0..N).step_by(2 * d) {
                    for axis in 0..d {
                        next[base + d + axis] += 0.5 * h * k1[base + d + axis];
                        next[base + axis] += h * next[base + d + axis];
                    }
                }
                let mut derivative = k1;
                system.system(next_time, &next, &mut derivative);
                for base in (0..N).step_by(2 * d) {
                    for axis in 0..d {
                        next[base + d + axis] += 0.5 * h * derivative[base + d + axis];
                    }
                }
                next
            }
            _ => unreachable!("adaptive algorithms use ode_solvers"),
        };
        let mut derivative = k1;
        system.system(next_time, &next, &mut derivative);
        let stop = next.iter().chain(derivative.iter()).any(|v| !v.is_finite())
            || system.solout(next_time, &next, &derivative);
        // Include the final candidate so model-specific validation can report
        // the boundary and retain the previous valid state, just as Sparse does.
        output.samples.push((next_time, next));
        if stop {
            return output;
        }
        state = next;
        time = next_time;
    }
    if time < end {
        output.result = Err(IntegrationError::MaxNumStepReached {
            x: time,
            n_step: request.budget,
        });
    }
    output
}

/// Shared controls keep fixed-step algorithms from advertising error control.
pub fn precision_controls(ui: &mut bevy_egui::egui::Ui, params: &mut FrameParams) {
    use bevy_egui::egui;
    let definition = params.algorithm.definition();
    ui.small(format!("{} · f64", definition.name));
    ui.add_enabled_ui(definition.adaptive, |ui| {
        ui.add(
            egui::Slider::new(&mut params.rtol, 1e-13..=1e-6)
                .logarithmic(true)
                .custom_formatter(|v, _| format!("{v:.0e}"))
                .text("Rel. tol"),
        );
        ui.add(
            egui::Slider::new(&mut params.atol, 1e-15..=1e-8)
                .logarithmic(true)
                .custom_formatter(|v, _| format!("{v:.0e}"))
                .text("Abs. tol"),
        );
    });
    ui.small(if definition.adaptive {
        "Tolerances control local error, not global accuracy."
    } else {
        "Tolerances are inactive. Steps use Max dt, shortened at frame boundaries. Reduce Max dt to check accuracy."
    });
}

#[cfg(test)]
mod tests;
