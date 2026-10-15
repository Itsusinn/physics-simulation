use std::sync::Arc;

use bevy::prelude::*;

use crate::{
    app::ThreeBodyApp,
    model::{FrameParams, HISTORY, InitialConditions},
    simulation::Simulation,
};

/// Immutable display data shared with the render world. No physics lives there.
pub struct DisplayFrame {
    pub state: Vec<u8>,
    pub history: Vec<u8>,
    pub revision: u64,
}

#[derive(Resource)]
pub struct Laboratory {
    pub simulation: Simulation,
    history: Vec<u8>,
    next: u32,
    count: u32,
    pub frame: Arc<DisplayFrame>,
}

impl Default for Laboratory {
    fn default() -> Self {
        let mut lab = Self {
            simulation: Simulation::new(InitialConditions::default(), 0).unwrap(),
            history: vec![0; HISTORY as usize * 3 * 16],
            next: 0,
            count: 0,
            frame: Arc::new(DisplayFrame {
                state: Vec::new(),
                history: Vec::new(),
                revision: 0,
            }),
        };
        lab.sample();
        lab.publish();
        lab
    }
}

impl Laboratory {
    pub fn reset(
        &mut self,
        initial: InitialConditions,
        generation: u64,
    ) -> Result<(), &'static str> {
        let simulation = Simulation::new(initial, generation)?;
        self.simulation = simulation;
        self.history.fill(0);
        self.next = 0;
        self.count = 0;
        self.sample();
        self.publish();
        Ok(())
    }

    pub fn advance(&mut self, params: &FrameParams) {
        let before = self.simulation.snapshot.time;
        self.simulation.advance(params);
        if self.simulation.snapshot.time > before {
            self.sample();
            self.publish();
        }
    }

    fn sample(&mut self) {
        for (i, body) in self.simulation.state.bodies.iter().enumerate() {
            let offset = (i * HISTORY as usize + self.next as usize) * 16;
            for (axis, value) in body.position.iter().enumerate() {
                self.history[offset + axis * 4..offset + axis * 4 + 4]
                    .copy_from_slice(&(*value as f32).to_le_bytes());
            }
        }
        self.next = (self.next + 1) % HISTORY;
        self.count = (self.count + 1).min(HISTORY);
    }

    fn publish(&mut self) {
        self.frame = Arc::new(DisplayFrame {
            state: self.simulation.state.render_bytes(self.next, self.count),
            history: self.history.clone(),
            revision: self.frame.revision.wrapping_add(1),
        });
    }
}

/// Runs once in Bevy Update, independently of egui's multi-pass layout and
/// render callbacks. Uncomputed time is dropped, never accumulated.
pub fn advance_simulation(delta_seconds: f64, lab: &mut Laboratory, ui: &mut ThreeBodyApp) {
    if lab.simulation.snapshot.generation != ui.generation {
        // Initial conditions are validated by restart before changing generation.
        lab.reset(ui.active.clone(), ui.generation)
            .expect("restart must validate the numerical state");
    }
    let mut params = ui.params.clone();
    params.duration = if std::mem::take(&mut ui.step) {
        params.max_step
    } else if ui.running {
        delta_seconds.clamp(0.0, 0.05) * f64::from(ui.speed)
    } else {
        0.0
    };
    lab.advance(&params);
    ui.read_diagnostics(lab.simulation.snapshot.clone());
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
