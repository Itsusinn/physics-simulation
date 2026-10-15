use ode_solvers::{SVector, System, dop_shared::IntegrationError};

use crate::integrators::{self, Algorithm, Request};

use crate::model::FrameParams;

pub type State = SVector<f64, 4>;
pub const MAX_ATTEMPTS: u32 = 2048;
// A numerical stopping boundary, not a physical collision radius or softening.
pub const MIN_RADIUS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    DoublePendulum,
    CoupledSprings,
    KeplerOrbit,
}

impl Kind {
    pub fn scene_id(self) -> &'static str {
        match self {
            Self::DoublePendulum => "2d-double-pendulum",
            Self::CoupledSprings => "1d-coupled-springs",
            Self::KeplerOrbit => "2d-kepler-orbit",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::DoublePendulum => "Double pendulum",
            Self::CoupledSprings => "Coupled spring oscillators",
            Self::KeplerOrbit => "Kepler orbit",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::DoublePendulum => {
                "Two point masses on massless rigid rods. Angles are measured from downward vertical, in radians. No damping or collisions."
            }
            Self::CoupledSprings => {
                "Two masses, two equal outer springs and one coupling spring. Displacements are relative to equilibrium. Ideal linear springs; no damping or collisions."
            }
            Self::KeplerOrbit => {
                "A test particle in a fixed inverse-square central field. mu = G M; energy and angular momentum are per unit particle mass. No softening."
            }
        }
    }

    pub fn parameter_names(self) -> &'static [&'static str] {
        match self {
            Self::DoublePendulum => &["Mass 1", "Mass 2", "Length 1", "Length 2", "Gravity g"],
            Self::CoupledSprings => &["Mass 1", "Mass 2", "Outer spring k", "Coupling k"],
            Self::KeplerOrbit => &["Gravity mu"],
        }
    }

    pub fn state_names(self) -> [&'static str; 4] {
        match self {
            Self::DoublePendulum => ["Angle 1 (rad)", "Angle 2 (rad)", "Omega 1", "Omega 2"],
            Self::CoupledSprings => [
                "Displacement 1",
                "Displacement 2",
                "Velocity 1",
                "Velocity 2",
            ],
            Self::KeplerOrbit => ["Position x", "Position y", "Velocity x", "Velocity y"],
        }
    }

    pub fn presets(self) -> &'static [&'static str] {
        match self {
            Self::DoublePendulum => &["Large-angle motion", "Small-angle motion"],
            Self::CoupledSprings => &["Energy exchange", "In-phase mode", "Opposite-phase mode"],
            Self::KeplerOrbit => &["Elliptical orbit", "Circular orbit", "Escape trajectory"],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub kind: Kind,
    pub parameters: [f64; 5],
    // Generalized positions followed by their velocities.
    pub initial: State,
}

impl Model {
    pub fn preset(kind: Kind, index: usize) -> Self {
        let (parameters, initial) = match kind {
            Kind::DoublePendulum => (
                [1.0, 1.0, 1.0, 1.0, 9.81],
                if index == 1 {
                    [0.15, 0.0, 0.0, 0.0]
                } else {
                    [2.1, 1.3, 0.0, 0.0]
                },
            ),
            Kind::CoupledSprings => (
                [1.0, 1.0, 1.0, 0.25, 0.0],
                [
                    1.0,
                    match index {
                        1 => 1.0,
                        2 => -1.0,
                        _ => 0.0,
                    },
                    0.0,
                    0.0,
                ],
            ),
            Kind::KeplerOrbit => (
                [1.0, 0.0, 0.0, 0.0, 0.0],
                [
                    1.0,
                    0.0,
                    0.0,
                    match index {
                        1 => 1.0,
                        2 => 1.5,
                        _ => 0.75,
                    },
                ],
            ),
        };
        Self {
            kind,
            parameters,
            initial: State::from(initial),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        for (i, value) in self.parameters[..self.kind.parameter_names().len()]
            .iter()
            .enumerate()
        {
            let allow_zero = self.kind == Kind::CoupledSprings && i == 3;
            if !value.is_finite() || *value > 100.0 || *value < if allow_zero { 0.0 } else { 0.01 }
            {
                return Err(
                    "Masses, lengths and constants must be in [0.01, 100]; coupling may be zero.",
                );
            }
        }
        if self
            .initial
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 100.0)
        {
            return Err("Initial coordinates and velocities must be finite and within +/-100.");
        }
        if !self.valid_state(&self.initial) {
            return Err(
                "Initial state is singular or outside the numerical range (Kepler r must exceed 1e-6).",
            );
        }
        Ok(())
    }

    pub fn derivative(&self, y: &State) -> State {
        let [q1, q2, v1, v2] = [y[0], y[1], y[2], y[3]];
        let [a, b, c, d, g] = self.parameters;
        let (acc1, acc2) = match self.kind {
            Kind::DoublePendulum => {
                // Euler-Lagrange mass matrix, angles from downward vertical.
                // Equivalent to https://www.myphysicslab.com/pendulum/double-pendulum-en.html
                let delta = q1 - q2;
                let cross = b * c * d * delta.cos();
                let m11 = (a + b) * c * c;
                let m22 = b * d * d;
                let f1 = -b * c * d * delta.sin() * v2 * v2 - (a + b) * g * c * q1.sin();
                let f2 = b * c * d * delta.sin() * v1 * v1 - b * g * d * q2.sin();
                // Expanded determinant avoids subtracting nearly equal terms.
                let det = b * c * c * d * d * (a + b * delta.sin().powi(2));
                ((f1 * m22 - cross * f2) / det, (m11 * f2 - cross * f1) / det)
            }
            Kind::CoupledSprings => ((-c * q1 + d * (q2 - q1)) / a, (-c * q2 + d * (q1 - q2)) / b),
            Kind::KeplerOrbit => {
                let r = q1.hypot(q2);
                let factor = -a / r.powi(3);
                (factor * q1, factor * q2)
            }
        };
        State::from([v1, v2, acc1, acc2])
    }

    pub fn energy_parts(&self, y: &State) -> [f64; 2] {
        let [q1, q2, v1, v2] = [y[0], y[1], y[2], y[3]];
        let [a, b, c, d, g] = self.parameters;
        match self.kind {
            Kind::DoublePendulum => [
                0.5 * (a + b) * c * c * v1 * v1
                    + 0.5 * b * d * d * v2 * v2
                    + b * c * d * v1 * v2 * (q1 - q2).cos(),
                (a + b) * g * c * (1.0 - q1.cos()) + b * g * d * (1.0 - q2.cos()),
            ],
            Kind::CoupledSprings => [
                0.5 * (a * v1 * v1 + b * v2 * v2),
                0.5 * c * (q1 * q1 + q2 * q2) + 0.5 * d * (q2 - q1).powi(2),
            ],
            Kind::KeplerOrbit => [0.5 * (v1 * v1 + v2 * v2), -a / q1.hypot(q2)],
        }
    }

    pub fn energy(&self, y: &State) -> f64 {
        self.energy_parts(y).iter().sum()
    }

    pub fn positions(&self, y: &State) -> [[f64; 2]; 2] {
        match self.kind {
            Kind::DoublePendulum => {
                let p = [
                    self.parameters[2] * y[0].sin(),
                    -self.parameters[2] * y[0].cos(),
                ];
                [
                    p,
                    [
                        p[0] + self.parameters[3] * y[1].sin(),
                        p[1] - self.parameters[3] * y[1].cos(),
                    ],
                ]
            }
            Kind::CoupledSprings => [[-2.0 + y[0], 0.0], [2.0 + y[1], 0.0]],
            Kind::KeplerOrbit => [[0.0, 0.0], [y[0], y[1]]],
        }
    }

    fn valid_state(&self, y: &State) -> bool {
        y.iter().all(|v| v.is_finite() && v.abs() <= 1e12)
            && (self.kind != Kind::KeplerOrbit || y[0].hypot(y[1]) > MIN_RADIUS)
            && self.energy(y).is_finite()
            && self.derivative(y).iter().all(|v| v.is_finite())
    }
}

impl System<f64, State> for Model {
    fn system(&self, _time: f64, y: &State, dy: &mut State) {
        *dy = self.derivative(y);
    }

    fn solout(&mut self, _time: f64, y: &State, _dy: &State) -> bool {
        !self.valid_state(y)
    }
}

pub struct Simulation {
    pub model: Model,
    pub state: State,
    pub time: f64,
    pub initial_energy: f64,
    pub energy_scale: f64,
    pub error: Option<String>,
    pub limited: bool,
    pub steps: usize,
    next_step: f64,
    last_algorithm: Algorithm,
}

impl Simulation {
    pub fn new(model: Model) -> Result<Self, &'static str> {
        model.validate()?;
        let initial_energy = model.energy(&model.initial);
        let energy_scale = model
            .energy_parts(&model.initial)
            .iter()
            .map(|e| e.abs())
            .sum::<f64>()
            .max(1e-15);
        Ok(Self {
            state: model.initial,
            model,
            time: 0.0,
            initial_energy,
            energy_scale,
            error: None,
            limited: false,
            steps: 0,
            next_step: 0.0,
            last_algorithm: Algorithm::default(),
        })
    }

    pub fn advance(&mut self, duration: f64, params: &FrameParams) {
        self.advance_with_budget(duration, params, MAX_ATTEMPTS);
    }

    fn advance_with_budget(&mut self, duration: f64, params: &FrameParams, budget: u32) {
        if !params.algorithm.supports(self.model.kind.scene_id()) {
            self.error = Some("Algorithm does not support this scene.".into());
            return;
        }
        if self.last_algorithm != params.algorithm {
            self.next_step = 0.0;
            self.last_algorithm = params.algorithm;
        }
        self.steps = 0;
        self.limited = false;
        if self.error.is_some() || duration == 0.0 {
            return;
        }
        if !params.valid_integration_inputs(duration) {
            self.error = Some("Duration, step and tolerances must be finite and positive.".into());
            return;
        }
        let end = self.time + duration;
        if !end.is_finite() || end <= self.time {
            self.error = Some("Time cannot advance at f64 precision. Reset to recover.".into());
            return;
        }
        let output = integrators::integrate(
            self.model.clone(),
            self.state,
            Request {
                params,
                start: self.time,
                duration,
                next_step: self.next_step,
                budget,
                position_dimensions: (self.model.kind != Kind::DoublePendulum).then_some(2),
            },
        );
        for (time, y) in &output.samples {
            let time = *time;
            if !time.is_finite() || time <= self.time || !self.model.valid_state(y) {
                self.error = Some("Numerical boundary reached (Kepler r <= 1e-6 or state out of range). Reset to recover.".into());
                return;
            }
            self.next_step = time - self.time;
            self.time = time;
            self.state = *y;
            self.steps += 1;
        }
        match output.result {
            Ok(_) if self.time >= end => {}
            Err(IntegrationError::MaxNumStepReached { .. }) if self.steps > 0 => {
                self.limited = true
            }
            Err(error) => {
                self.error = Some(format!("Integrator stopped: {error}. Reset to recover."))
            }
            Ok(_) => {
                self.error =
                    Some("Integration stopped before the requested time. Reset to recover.".into())
            }
        }
    }
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
