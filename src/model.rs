pub const HISTORY: u32 = 4096;
pub const STATE_BYTES: u64 = 144;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub mass: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InitialConditions {
    pub bodies: [Body; 3],
    pub gravity: f32,
    pub softening: f32,
}

impl Default for InitialConditions {
    fn default() -> Self {
        Self {
            // Each mass is opposite the side of the same length. Center of mass = (0, 0).
            bodies: [
                Body {
                    position: [1.0, 3.0],
                    velocity: [0.0; 2],
                    mass: 3.0,
                },
                Body {
                    position: [-2.0, -1.0],
                    velocity: [0.0; 2],
                    mass: 4.0,
                },
                Body {
                    position: [1.0, -1.0],
                    velocity: [0.0; 2],
                    mass: 5.0,
                },
            ],
            gravity: 1.0,
            softening: 0.0,
        }
    }
}

impl InitialConditions {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.gravity.is_finite()
            || self.gravity <= 0.0
            || !self.softening.is_finite()
            || self.softening < 0.0
        {
            return Err("G must be positive and softening must be nonnegative.");
        }
        for body in self.bodies {
            if !body.mass.is_finite()
                || body.mass <= 0.0
                || body
                    .position
                    .into_iter()
                    .chain(body.velocity)
                    .any(|v| !v.is_finite())
            {
                return Err("Masses must be positive; all coordinates must be finite.");
            }
        }
        if self.softening == 0.0 {
            for i in 0..3 {
                for j in (i + 1)..3 {
                    if self.bodies[i].position == self.bodies[j].position {
                        return Err("Coincident bodies require nonzero softening.");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn state_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(STATE_BYTES as usize);
        for body in self.bodies {
            for value in [
                body.position[0],
                body.position[1],
                body.velocity[0],
                body.velocity[1],
                body.mass,
                0.0,
                0.0,
                0.0,
            ] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes.resize(STATE_BYTES as usize, 0);
        bytes
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub positions: [[f32; 2]; 3],
    pub time: f32,
    pub initial_energy: f32,
    pub energy: f32,
    pub separation: f32,
    pub momentum: [f32; 2],
    pub angular_momentum: f32,
    pub min_step: f32,
    pub steps: u32,
    pub flags: u32,
    pub generation: u64,
    pub ready: bool,
    pub error: Option<String>,
}

impl Snapshot {
    pub fn decode(bytes: &[u8], generation: u64) -> Self {
        let f = |offset: usize| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let u = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        Self {
            positions: std::array::from_fn(|i| [f(i * 32), f(i * 32 + 4)]),
            time: f(96),
            initial_energy: f(100),
            energy: f(104),
            separation: f(108),
            momentum: [f(112), f(116)],
            angular_momentum: f(120),
            min_step: f(124),
            steps: u(136),
            flags: u(140),
            generation,
            ready: true,
            error: None,
        }
    }

    pub fn relative_energy_error(&self) -> f32 {
        (self.energy - self.initial_energy).abs() / self.initial_energy.abs().max(1e-8)
    }
}

#[derive(Clone)]
pub struct FrameParams {
    pub gravity: f32,
    pub softening: f32,
    pub max_step: f32,
    pub duration: f32,
    pub center: [f32; 2],
    pub scale: f32,
    pub viewport: [f32; 2],
    pub trail_opacity: f32,
    pub body_size: f32,
    pub trail_length: u32,
    pub grid: bool,
    pub adaptive: bool,
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            gravity: 1.0,
            softening: 0.0,
            max_step: 0.001,
            duration: 0.0,
            center: [0.0; 2],
            scale: 5.0,
            viewport: [1000.0, 800.0],
            trail_opacity: 0.8,
            body_size: 1.0,
            trail_length: 2048,
            grid: true,
            adaptive: true,
        }
    }
}

impl FrameParams {
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        for value in [
            self.gravity,
            self.softening,
            self.max_step,
            self.duration,
            self.center[0],
            self.center[1],
            self.scale,
            self.viewport[0] / self.viewport[1].max(1.0),
            self.viewport[0],
            self.viewport[1],
            self.trail_opacity,
            self.body_size,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            self.trail_length,
            u32::from(self.grid),
            u32::from(self.adaptive),
            0,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pythagorean_initial_conditions_have_correct_geometry_and_barycenter() {
        let initial = InitialConditions::default();
        for i in 0..3 {
            let a = initial.bodies[(i + 1) % 3].position;
            let b = initial.bodies[(i + 2) % 3].position;
            assert_eq!((a[0] - b[0]).hypot(a[1] - b[1]), initial.bodies[i].mass);
        }
        for axis in 0..2 {
            assert_eq!(
                initial
                    .bodies
                    .iter()
                    .map(|b| b.mass * b.position[axis])
                    .sum::<f32>(),
                0.0
            );
        }
        assert_eq!(initial.state_bytes().len(), STATE_BYTES as usize);
        assert_eq!(FrameParams::default().bytes().len(), 64);
    }

    #[test]
    fn singular_and_nonfinite_initial_conditions_are_rejected() {
        let mut initial = InitialConditions::default();
        initial.bodies[1].position = initial.bodies[0].position;
        assert!(initial.validate().is_err());
        initial.softening = 0.01;
        assert!(initial.validate().is_ok());
        initial.bodies[0].velocity[0] = f32::NAN;
        assert!(initial.validate().is_err());
    }
}
