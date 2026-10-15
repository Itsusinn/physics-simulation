use ode_solvers::{Dop853, OutputType, SVector, System, dop_shared::IntegrationError};

use crate::model::{FrameParams, InitialConditions, Snapshot};

pub const MAX_ATTEMPTS: u32 = 2048;
type State = SVector<f64, 12>;

struct Gravity<'a>(&'a InitialConditions);

impl System<f64, State> for Gravity<'_> {
    fn system(&self, _time: f64, y: &State, dy: &mut State) {
        dy.fill(0.0);
        for i in 0..3 {
            dy[4 * i] = y[4 * i + 2];
            dy[4 * i + 1] = y[4 * i + 3];
            for j in (i + 1)..3 {
                let dx = y[4 * j] - y[4 * i];
                let dz = y[4 * j + 1] - y[4 * i + 1];
                let r2 = dx * dx + dz * dz + self.0.softening.powi(2);
                let force = self.0.gravity / (r2 * r2.sqrt());
                for (axis, delta) in [dx, dz].into_iter().enumerate() {
                    dy[4 * i + 2 + axis] += delta * force * self.0.bodies[j].mass;
                    dy[4 * j + 2 + axis] -= delta * force * self.0.bodies[i].mass;
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
                next.bodies[i].position = [y[4 * i], y[4 * i + 1]];
                next.bodies[i].velocity = [y[4 * i + 2], y[4 * i + 3]];
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
        let b = state.bodies[i / 4];
        [b.position[0], b.position[1], b.velocity[0], b.velocity[1]][i % 4]
    })
}

fn valid(state: &InitialConditions, snapshot: &Snapshot) -> bool {
    state.bodies.iter().all(|b| {
        b.position.iter().all(|v| (*v as f32).is_finite())
            && b.velocity.iter().all(|v| v.is_finite())
            && (b.mass as f32).is_finite()
    }) && [
        snapshot.energy,
        snapshot.separation,
        snapshot.momentum[0],
        snapshot.momentum[1],
        snapshot.angular_momentum,
    ]
    .iter()
    .all(|v| v.is_finite())
}

fn diagnostics(state: &InitialConditions) -> Snapshot {
    let mut snapshot = Snapshot {
        positions: state.bodies.map(|b| b.position),
        separation: f64::INFINITY,
        ..Default::default()
    };
    for (i, b) in state.bodies.iter().enumerate() {
        snapshot.energy += 0.5 * b.mass * (b.velocity[0].powi(2) + b.velocity[1].powi(2));
        for axis in 0..2 {
            snapshot.momentum[axis] += b.mass * b.velocity[axis];
        }
        snapshot.angular_momentum +=
            b.mass * (b.position[0] * b.velocity[1] - b.position[1] * b.velocity[0]);
        for other in &state.bodies[i + 1..] {
            let r = (other.position[0] - b.position[0]).hypot(other.position[1] - b.position[1]);
            snapshot.separation = snapshot.separation.min(r);
            snapshot.energy -= state.gravity * b.mass * other.mass / r.hypot(state.softening);
        }
    }
    snapshot
}

#[cfg(test)]
#[path = "simulation_tests.rs"]
mod tests;
