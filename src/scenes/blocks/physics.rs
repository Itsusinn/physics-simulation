//! Event-driven one-dimensional rigid impacts. Free flight is linear and each
//! binary impact satisfies momentum conservation and Newton restitution in f64.
//! This ideal instantaneous-contact model does not simulate material deformation.

pub const SIZES: [f64; 2] = [0.7, 1.4];
pub const START_X: [f64; 2] = [2.0, 5.0];
const MAX_POSITION: f64 = 1e12;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub masses: [f64; 2],
    pub incoming_speed: f64,
    pub restitution: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            masses: [1.0, 100.0],
            incoming_speed: 1.0,
            restitution: 1.0,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.masses.iter().any(|&m| !m.is_finite() || m <= 0.0) {
            return Err("Masses must be finite positive numbers.");
        }
        let largest = self.masses[0].max(self.masses[1]);
        if self
            .masses
            .iter()
            .any(|&m| !m.is_normal() || !m.recip().is_normal() || !(m / largest).is_normal())
        {
            return Err("Mass or mass ratio exceeds the f64 solver range.");
        }
        if !self.incoming_speed.is_finite() || !(0.1..=3.0).contains(&self.incoming_speed) {
            return Err("Incoming speed must be from 0.1 to 3.");
        }
        if !self.restitution.is_finite() || !(0.0..=1.0).contains(&self.restitution) {
            return Err("Restitution must be from 0 to 1.");
        }
        if !energy(self.masses, [0.0, -self.incoming_speed]).is_normal() {
            return Err("Initial kinetic energy exceeds the f64 solver range.");
        }
        Ok(())
    }

    fn weights(&self) -> [f64; 2] {
        // Scale before adding, to avoid overflowing the total mass.
        let scale = self.masses[0].max(self.masses[1]);
        let scaled = self.masses.map(|m| m / scale);
        scaled.map(|m| m / (scaled[0] + scaled[1]))
    }
}

fn energy(masses: [f64; 2], velocity: [f64; 2]) -> f64 {
    masses
        .into_iter()
        .zip(velocity)
        .map(|(m, v)| (0.5 * m * v) * v)
        .sum()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub x: [f64; 2],
    pub velocity: [f64; 2],
    pub time: f64,
    pub kinetic: f64,
    pub dissipated: f64,
    pub collisions: [u64; 2], // block/block, left block/wall
}

impl Snapshot {
    pub fn total_collisions(&self) -> u64 {
        self.collisions.iter().sum()
    }
}

#[derive(Clone, Copy)]
struct State {
    // Surface gaps retain precision near contact instead of subtracting centers.
    gaps: [f64; 2], // wall/left block, left/right block
    velocity: [f64; 2],
    time: f64,
    dissipated: f64,
    collisions: [u64; 2],
}

impl State {
    fn flight(self, duration: f64) -> Self {
        Self {
            gaps: [
                self.velocity[0].mul_add(duration, self.gaps[0]),
                (self.velocity[1] - self.velocity[0]).mul_add(duration, self.gaps[1]),
            ],
            time: self.time + duration,
            ..self
        }
    }

    fn snapshot(self, masses: [f64; 2]) -> Snapshot {
        Snapshot {
            x: [
                self.gaps[0] + SIZES[0] * 0.5,
                self.gaps[0] + SIZES[0] + self.gaps[1] + SIZES[1] * 0.5,
            ],
            velocity: self.velocity,
            time: self.time,
            kinetic: energy(masses, self.velocity),
            dissipated: self.dissipated,
            collisions: self.collisions,
        }
    }
}

#[derive(Default, Debug)]
pub struct Advance {
    pub impacts: usize,
    pub budget_limited: bool,
}

pub struct Simulation {
    // Frames sample the trajectory anchored at the last impact. They never
    // re-anchor free flight, so event times do not depend on frame subdivision.
    anchor: State,
    pub settings: Settings,
    pub snapshot: Snapshot,
    pub initial_energy: f64,
    pub error: Option<String>,
}

impl Simulation {
    pub fn new(settings: Settings) -> Result<Self, &'static str> {
        settings.validate()?;
        let anchor = State {
            gaps: [
                START_X[0] - SIZES[0] * 0.5,
                START_X[1] - START_X[0] - (SIZES[0] + SIZES[1]) * 0.5,
            ],
            velocity: [0.0, -settings.incoming_speed],
            time: 0.0,
            dissipated: 0.0,
            collisions: [0; 2],
        };
        let snapshot = anchor.snapshot(settings.masses);
        Ok(Self {
            anchor,
            settings,
            initial_energy: snapshot.kinetic,
            snapshot,
            error: None,
        })
    }

    pub fn advance(&mut self, duration: f64, max_impacts: usize) -> Advance {
        let mut result = Advance::default();
        if self.error.is_some() || !duration.is_finite() || duration <= 0.0 {
            return result;
        }
        let target = self.snapshot.time + duration;
        if !target.is_finite() || target <= self.snapshot.time {
            self.error = Some(
                "Time cannot advance in f64. Last valid state retained; Reset to recover.".into(),
            );
            return result;
        }
        loop {
            let a = self.anchor;
            let wall = if a.velocity[0] < 0.0 {
                a.gaps[0] / -a.velocity[0]
            } else {
                f64::INFINITY
            };
            let blocks = if a.velocity[0] > a.velocity[1] {
                a.gaps[1] / (a.velocity[0] - a.velocity[1])
            } else {
                f64::INFINITY
            };
            let interval = wall.min(blocks);
            let event_time = a.time + interval;
            if event_time > target || !interval.is_finite() {
                let mut state = a.flight(target - a.time);
                state.time = target;
                self.publish(state);
                return result;
            }
            let simultaneous = wall == blocks;
            let impacts = if simultaneous { 2 } else { 1 };
            if max_impacts - result.impacts < impacts {
                // Drop uncomputed wall-clock time without skipping any impact.
                result.budget_limited = true;
                return result;
            }
            let closing_gap = if wall <= blocks { a.gaps[0] } else { a.gaps[1] };
            if (interval > 0.0 && event_time <= a.time) || (interval == 0.0 && closing_gap > 0.0) {
                self.error = Some("Impact times are below f64 resolution (possible inelastic collapse or extreme mass ratio). Last valid state retained; Reset to recover.".into());
                return result;
            }
            if simultaneous && self.settings.restitution > 0.0 {
                self.error = Some("Simultaneous wall and block impacts need a multi-contact material model. Last valid state retained; Reset to recover.".into());
                return result;
            }
            let mut state = a.flight(interval);
            let wall_hit = wall <= blocks;
            state.gaps[if wall_hit { 0 } else { 1 }] = 0.0;
            // Only remove roundoff at an analytically located contact. No
            // overlap correction impulse or minimum bounce speed is introduced.
            for gap in &mut state.gaps {
                if *gap < 0.0 && *gap >= -64.0 * f64::EPSILON * a.gaps[0].max(a.gaps[1]).max(1.0) {
                    *gap = 0.0;
                }
            }
            if state.gaps == [0.0, 0.0] && self.settings.restitution > 0.0 {
                self.error = Some("Simultaneous wall and block contacts need a multi-contact material model (possible inelastic collapse). Last valid state retained; Reset to recover.".into());
                return result;
            }
            let e = self.settings.restitution;
            let [u, v] = a.velocity;
            if e == 0.0 && state.gaps == [0.0, 0.0] {
                // Persistent completely inelastic contacts transmit the wall
                // impulse through the pair without inventing repeated impacts.
                state.velocity = [0.0; 2];
                state.dissipated += energy(self.settings.masses, a.velocity);
            } else if wall_hit {
                state.velocity[0] = -e * u;
                state.dissipated += (0.5 * self.settings.masses[0] * u) * u * (1.0 - e) * (1.0 + e);
            } else {
                let [w0, w1] = self.settings.weights();
                if e == 0.0 {
                    let common = w0.mul_add(u, w1 * v);
                    state.velocity = [common; 2];
                } else {
                    state.velocity = [
                        (-(1.0 + e) * w1).mul_add(u - v, u),
                        ((1.0 + e) * w0).mul_add(u - v, v),
                    ];
                }
                let reduced_mass = if self.settings.masses[0] <= self.settings.masses[1] {
                    self.settings.masses[0] * w1
                } else {
                    self.settings.masses[1] * w0
                };
                state.dissipated +=
                    (0.5 * reduced_mass * (u - v)) * (u - v) * (1.0 - e) * (1.0 + e);
            }
            let index = usize::from(wall_hit);
            // Reserve space for total_collisions() as well as each subtotal.
            if state.collisions[0]
                .checked_add(state.collisions[1])
                .and_then(|n| n.checked_add(impacts as u64))
                .is_none()
            {
                self.error = Some(
                    "Collision counter exhausted. Last valid state retained; Reset to recover."
                        .into(),
                );
                return result;
            }
            state.collisions[index] += 1;
            if simultaneous {
                state.collisions[1 - index] += 1;
            }
            if !self.publish(state) {
                return result;
            }
            self.anchor = state;
            result.impacts += impacts;
        }
    }

    fn publish(&mut self, state: State) -> bool {
        let snapshot = state.snapshot(self.settings.masses);
        if state.gaps.iter().any(|&gap| !gap.is_finite() || gap < 0.0)
            || snapshot
                .x
                .iter()
                .chain(snapshot.velocity.iter())
                .any(|v| !v.is_finite() || v.abs() > MAX_POSITION)
            || !snapshot.kinetic.is_finite()
            || !snapshot.dissipated.is_finite()
        {
            self.error = Some("Collision state exceeds the numerical/display range. Last valid state retained; Reset to recover.".into());
            return false;
        }
        self.snapshot = snapshot;
        true
    }
}

#[cfg(test)]
#[path = "physics/tests.rs"]
mod tests;
