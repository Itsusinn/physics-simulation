use bevy_egui::egui;

use super::Scene;
use crate::{
    app::ThreeBodyApp,
    runtime::{Laboratory, advance_simulation},
};

#[derive(Default)]
struct ThreeBodyScene {
    app: ThreeBodyApp,
    lab: Laboratory,
}

pub(super) fn create() -> Box<dyn Scene> {
    Box::<ThreeBodyScene>::default()
}

impl Scene for ThreeBodyScene {
    fn algorithm(&self) -> Option<crate::integrators::Algorithm> {
        Some(self.app.params.algorithm)
    }

    fn set_algorithm(
        &mut self,
        algorithm: crate::integrators::Algorithm,
    ) -> Result<(), &'static str> {
        self.app.params.set_algorithm(algorithm, "2d-345-threebody")
    }

    fn advance(&mut self, delta_seconds: f64) {
        advance_simulation(delta_seconds, &mut self.lab, &mut self.app);
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        self.app.ui(ui, self.lab.frame.clone());
    }

    fn is_running(&self) -> bool {
        self.app.running
    }
}
