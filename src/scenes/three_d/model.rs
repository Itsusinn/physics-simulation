#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub mass: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InitialConditions {
    pub bodies: [Body; 3],
    pub gravity: f64,
    pub softening: f64,
}

impl InitialConditions {
    pub fn classic() -> Self {
        let planar = crate::model::InitialConditions::default();
        Self {
            bodies: planar.bodies.map(|body| Body {
                position: [body.position[0], body.position[1], 0.0],
                velocity: [0.0; 3],
                mass: body.mass,
            }),
            gravity: 1.0,
            softening: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.gravity.is_finite()
            || self.gravity <= 0.0
            || !self.softening.is_finite()
            || self.softening < 0.0
        {
            return Err("G must be positive and softening must be nonnegative.");
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if !body.mass.is_finite()
                || body.mass <= 0.0
                || body
                    .position
                    .iter()
                    .chain(body.velocity.iter())
                    .any(|v| !v.is_finite())
            {
                return Err("Masses must be positive; all coordinates must be finite.");
            }
            if self.softening == 0.0
                && self.bodies[i + 1..]
                    .iter()
                    .any(|other| other.position == body.position)
            {
                return Err("Coincident bodies require nonzero softening.");
            }
        }
        Ok(())
    }
}

impl Default for InitialConditions {
    fn default() -> Self {
        let mut state = Self::classic();
        // Spatial perturbation of 3-4-5, with zero center of mass and momentum.
        for (body, (z, velocity)) in state.bodies.iter_mut().zip([
            (0.4, [0.0, 0.0, 0.18]),
            (-0.45, [0.12, 0.0, -0.1]),
            (0.12, [-0.096, 0.0, -0.028]),
        ]) {
            body.position[2] = z;
            body.velocity = velocity;
        }
        state
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub time: f64,
    pub initial_energy: f64,
    pub energy: f64,
    pub separation: f64,
    pub momentum: [f64; 3],
    pub angular_momentum: [f64; 3],
    pub min_step: f64,
    pub steps: u32,
    pub flags: u32,
    pub generation: u64,
    pub ready: bool,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn relative_energy_error(&self) -> f64 {
        (self.energy - self.initial_energy).abs() / self.initial_energy.abs().max(1e-30)
    }
}
