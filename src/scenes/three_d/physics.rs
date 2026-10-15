use ode_solvers::{Dop853, OutputType, SVector, System, dop_shared::IntegrationError};

use super::model::{InitialConditions, Snapshot};
use crate::model::FrameParams;

pub const MAX_ATTEMPTS: u32 = 2048;
type State = SVector<f64, 18>;

struct Gravity<'a>(&'a InitialConditions);

impl System<f64, State> for Gravity<'_> {
    fn system(&self, _time: f64, y: &State, dy: &mut State) {
        dy.fill(0.0);
        for i in 0..3 {
            for axis in 0..3 {
                dy[6 * i + axis] = y[6 * i + 3 + axis];
            }
            for j in (i + 1)..3 {
                let delta: [f64; 3] = std::array::from_fn(|axis| y[6 * j + axis] - y[6 * i + axis]);
                let r2 = delta.iter().map(|v| v * v).sum::<f64>() + self.0.softening.powi(2);
                let force = self.0.gravity / (r2 * r2.sqrt());
                for (axis, delta) in delta.into_iter().enumerate() {
                    dy[6 * i + 3 + axis] += delta * force * self.0.bodies[j].mass;
                    dy[6 * j + 3 + axis] -= delta * force * self.0.bodies[i].mass;
                }
            }
        }
    }
    fn solout(&mut self, _time: f64, y: &State, dy: &State) -> bool {
        // Stop immediately if an accepted step leaves the representable domain.
        y.iter().chain(dy.iter()).any(|v| !v.is_finite())
    }
}

/// The authoritative physical state. Rendering never feeds f32 data back into it.
pub struct Simulation {
    pub state: InitialConditions,
    pub snapshot: Snapshot,
    next_step: f64,
}

impl Simulation {
    pub fn new(initial: InitialConditions, generation: u64) -> Result<Self, &'static str> {
        initial.validate()?;
        let mut snapshot = diagnostics(&initial);
        let y = vector(&initial);
        let mut dy = State::zeros();
        Gravity(&initial).system(0.0, &y, &mut dy);
        if !valid(&initial, &snapshot) || dy.iter().any(|v| !v.is_finite()) {
            return Err("Initial conditions exceed the numerical or display range.");
        }
        snapshot.initial_energy = snapshot.energy;
        snapshot.generation = generation;
        snapshot.ready = true;
        snapshot.flags = 1;
        Ok(Self {
            state: initial,
            snapshot,
            next_step: 0.0,
        })
    }

    pub fn advance(&mut self, params: &FrameParams) {
        if self.snapshot.error.is_some() {
            return;
        }
        self.snapshot.steps = 0;
        self.snapshot.min_step = 0.0;
        self.snapshot.flags = 1;
        if params.duration == 0.0 {
            return;
        }
        if [params.duration, params.max_step, params.rtol, params.atol]
            .into_iter()
            .any(|v| !v.is_finite() || v <= 0.0)
        {
            self.fail("Duration, Max dt and tolerances must be finite and positive.");
            return;
        }
        let start = self.snapshot.time;
        let end = start + params.duration;
        if !end.is_finite() || end <= start {
            self.fail("Time cannot advance at f64 precision. Reset the simulation.");
            return;
        }
        let mut solver = Dop853::from_param(
            Gravity(&self.state),
            start,
            end,
            params.duration,
            vector(&self.state),
            params.rtol,
            params.atol,
            0.9,
            0.0,
            0.333,
            6.0,
            params.max_step.min(params.duration),
            self.next_step.min(params.max_step).min(params.duration),
            MAX_ATTEMPTS,
            1000,
            OutputType::Sparse,
        );
        let result = solver.integrate();
        // Sparse output includes the initial state and every accepted step, even
        // when the attempt budget is exhausted. Never install a rejected stage.
        let mut last = self.snapshot.clone();
        let mut candidate = self.state.clone();
        let mut min_step = f64::INFINITY;
        let mut failed = false;
        for (&time, y) in solver.x_out().iter().zip(solver.y_out()).skip(1) {
            let mut next = candidate.clone();
            for i in 0..3 {
                next.bodies[i].position = std::array::from_fn(|axis| y[6 * i + axis]);
                next.bodies[i].velocity = std::array::from_fn(|axis| y[6 * i + 3 + axis]);
            }
            let mut metrics = diagnostics(&next);
            let mut derivative = State::zeros();
            Gravity(&next).system(time, y, &mut derivative);
            if !time.is_finite()
                || time <= last.time
                || !valid(&next, &metrics)
                || derivative.iter().any(|v| !v.is_finite())
            {
                failed = true;
                break;
            }
            let dt = time - last.time;
            min_step = min_step.min(dt);
            self.next_step = dt;
            metrics.time = time;
            metrics.initial_energy = last.initial_energy;
            metrics.generation = last.generation;
            metrics.steps = last.steps + 1;
            metrics.min_step = min_step;
            metrics.ready = true;
            metrics.flags = 1;
            last = metrics;
            candidate = next;
        }
        self.state = candidate;
        self.snapshot = last;
        if failed {
            self.fail("Integration left the numerical or display range. Reset the simulation.");
        } else {
            match result {
                Ok(_) if self.snapshot.time >= end => {}
                Ok(_) => self.fail("DOP853 stopped before reaching the requested time."),
                Err(IntegrationError::MaxNumStepReached { .. }) if self.snapshot.time == start => {
                    self.fail("DOP853 could not accept a step within the work budget. Reset the simulation.");
                }
                Err(IntegrationError::MaxNumStepReached { .. }) => {
                    // Drop uncomputed frame time; do not increase dt to catch up.
                    self.snapshot.flags |= 2;
                }
                Err(error) => self.fail(&format!("DOP853 stopped: {error}")),
            }
        }
    }

    fn fail(&mut self, message: &str) {
        self.snapshot.flags |= 4;
        self.snapshot.error = Some(message.into());
    }
}

fn vector(state: &InitialConditions) -> State {
    State::from_fn(|i, _| {
        let body = state.bodies[i / 6];
        if i % 6 < 3 {
            body.position[i % 6]
        } else {
            body.velocity[i % 6 - 3]
        }
    })
}

fn valid(state: &InitialConditions, snapshot: &Snapshot) -> bool {
    state.bodies.iter().all(|body| {
        body.position
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e12)
            && body.velocity.iter().all(|v| v.is_finite())
            && (body.mass as f32).is_finite()
    }) && [snapshot.energy, snapshot.separation]
        .iter()
        .chain(snapshot.momentum.iter())
        .chain(snapshot.angular_momentum.iter())
        .all(|v| v.is_finite())
}

fn diagnostics(state: &InitialConditions) -> Snapshot {
    let mut snapshot = Snapshot {
        separation: f64::INFINITY,
        ..Default::default()
    };
    for (i, body) in state.bodies.iter().enumerate() {
        snapshot.energy += 0.5 * body.mass * body.velocity.iter().map(|v| v * v).sum::<f64>();
        for axis in 0..3 {
            snapshot.momentum[axis] += body.mass * body.velocity[axis];
            let j = (axis + 1) % 3;
            let k = (axis + 2) % 3;
            snapshot.angular_momentum[axis] += body.mass
                * (body.position[j] * body.velocity[k] - body.position[k] * body.velocity[j]);
        }
        for other in &state.bodies[i + 1..] {
            let r = (0..3)
                .map(|axis| (other.position[axis] - body.position[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            snapshot.separation = snapshot.separation.min(r);
            snapshot.energy -= state.gravity * body.mass * other.mass / r.hypot(state.softening);
        }
    }
    snapshot
}

#[cfg(test)]
#[path = "physics_tests.rs"]
mod tests;
