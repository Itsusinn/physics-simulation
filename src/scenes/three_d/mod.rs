mod camera;
mod model;
mod physics;
mod render;

use std::{collections::VecDeque, sync::Arc};

use bevy_egui::{
    egui::{self, Color32, RichText},
    render::EguiBevyPaintCallback,
};

use super::Scene;
use crate::model::{FrameParams, HISTORY};
use camera::Camera;
use model::InitialConditions;
use physics::Simulation;
use render::{Geometry, Orbit3dCallback};

pub(super) fn create() -> Box<dyn Scene> {
    Box::new(ThreeBody3d::default())
}

struct ThreeBody3d {
    simulation: Simulation,
    active: InitialConditions,
    draft: InitialConditions,
    pending_reset: Option<InitialConditions>,
    params: FrameParams,
    running: bool,
    step: bool,
    speed: f32,
    history: VecDeque<[[f64; 3]; 3]>,
    frame: Arc<Geometry>,
    camera: Camera,
    auto_frame: bool,
    fit_requested: bool,
    show_controls: bool,
    initialized: bool,
    validation_error: Option<String>,
}

impl Default for ThreeBody3d {
    fn default() -> Self {
        let initial = InitialConditions::default();
        let history = VecDeque::from([initial.bodies.map(|body| body.position)]);
        Self {
            simulation: Simulation::new(initial.clone(), 0).expect("valid 3D preset"),
            frame: Arc::new(Geometry::new(&initial, &history)),
            active: initial.clone(),
            draft: initial,
            history,
            pending_reset: None,
            params: FrameParams::default(),
            running: false,
            step: false,
            speed: 1.0,
            camera: Camera::default(),
            auto_frame: false,
            fit_requested: true,
            show_controls: true,
            initialized: false,
            validation_error: None,
        }
    }
}

impl ThreeBody3d {
    fn restart(&mut self, initial: InitialConditions) {
        if let Err(error) = Simulation::new(initial.clone(), 0) {
            self.validation_error = Some(error.into());
            return;
        }
        self.pending_reset = Some(initial);
        self.running = false;
        self.step = false;
        self.validation_error = None;
    }

    fn controls(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("3d-parameters")
            .exact_size(330.0_f32.min((root.available_width() - 10.0).max(200.0)))
            .resizable(false).show(root, |ui| {
                ui.spacing_mut().slider_width = (ui.available_width() - 175.0).clamp(45.0, 115.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.heading("Spatial three-body system");
                    ui.label("f64 · 3 dimensions");
                    ui.horizontal(|ui| {
                        if ui.button(if self.running { "Pause" } else { "Run" }).clicked() { self.running = !self.running; }
                        if ui.add_enabled(!self.running, egui::Button::new("Step")).clicked() { self.step = true; }
                        if ui.button("Reset").clicked() { self.restart(self.active.clone()); }
                    });
                    ui.small("Space: run / pause · R: reset");
                    ui.separator();
                    ui.add(egui::Slider::new(&mut self.speed, 0.05..=10.0).logarithmic(true).text("Speed"));
                    ui.add(egui::Slider::new(&mut self.params.max_step, 0.00001..=0.1).logarithmic(true).text("Max dt"));
                    crate::integrators::precision_controls(ui, &mut self.params);
                    ui.separator();
                    ui.strong("Initial conditions");
                    ui.small("Changes take effect on Apply.");
                    ui.add(egui::Slider::new(&mut self.draft.gravity, 0.01..=10.0).logarithmic(true).text("Gravity G"));
                    ui.add(egui::Slider::new(&mut self.draft.softening, 0.0..=0.1).max_decimals(4).text("Softening"));
                    if self.draft.softening > 0.0 { ui.small("Softening changes the Newtonian model."); }
                    for (i, body) in self.draft.bodies.iter_mut().enumerate() {
                        egui::CollapsingHeader::new(format!("Body {}", i + 1)).id_salt(i).default_open(i == 0).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Mass");
                                ui.add(egui::DragValue::new(&mut body.mass).speed(0.05).range(0.01..=100.0));
                            });
                            egui::Grid::new(("3d-initial", i)).num_columns(3).show(ui, |ui| {
                                ui.label("Axis"); ui.label("Position"); ui.label("Velocity"); ui.end_row();
                                for (axis, name) in ["x", "y", "z"].into_iter().enumerate() {
                                    ui.label(name);
                                    ui.add(egui::DragValue::new(&mut body.position[axis]).speed(0.02).range(-100.0..=100.0).max_decimals(3));
                                    ui.add(egui::DragValue::new(&mut body.velocity[axis]).speed(0.01).range(-100.0..=100.0).max_decimals(3));
                                    ui.end_row();
                                }
                            });
                        });
                    }
                    if ui.add_enabled(self.draft != self.active, egui::Button::new("Apply & restart")).clicked() {
                        self.restart(self.draft.clone());
                    }
                    if ui.button("Restore spatial preset").clicked() {
                        self.draft = InitialConditions::default(); self.restart(self.draft.clone());
                    }
                    if ui.button("Classic 3-4-5 in XY plane").clicked() {
                        self.draft = InitialConditions::classic(); self.restart(self.draft.clone());
                    }
                    if let Some(error) = &self.validation_error { ui.colored_label(Color32::LIGHT_RED, error); }
                    ui.separator();
                    ui.strong("View");
                    ui.add(egui::Slider::new(&mut self.params.trail_length, 32..=4096).text("Trail samples"));
                    ui.add(egui::Slider::new(&mut self.params.trail_opacity, 0.0..=1.0).text("Trail opacity"));
                    ui.add(egui::Slider::new(&mut self.params.body_size, 0.5..=2.0).text("Body size"));
                    ui.checkbox(&mut self.params.grid, "XY grid & XYZ axes");
                    ui.checkbox(&mut self.auto_frame, "Auto frame");
                    ui.horizontal(|ui| {
                        if ui.button("Fit all bodies").clicked() { self.fit_requested = true; }
                        if ui.button("Reset camera").clicked() { self.camera = Camera::default(); self.fit_requested = true; }
                    });
                    ui.small("Drag: orbit · Shift-drag / right-drag: pan\nScroll: zoom · Double-click: fit");
                    ui.small("Axes: X red · Y green · Z blue. Body size is for display only.");
                    ui.separator();
                    let snapshot = &self.simulation.snapshot;
                    ui.strong("Conservation");
                    ui.monospace(format!("Energy  {:+.8}", snapshot.energy));
                    for (name, values) in [("P", snapshot.momentum), ("L", snapshot.angular_momentum)] {
                        ui.monospace(format!("{name}x {:+.2e}  {name}y {:+.2e}", values[0], values[1]));
                        ui.monospace(format!("{name}z {:+.2e}", values[2]));
                    }
                    ui.small("Chaotic trajectories amplify numerical errors. Compare tolerances when exploring close encounters.");
                });
            });
    }

    fn viewport(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                let (rect, response) = ui.allocate_exact_size(
                    ui.available_size().max(egui::Vec2::splat(1.0)),
                    egui::Sense::click_and_drag(),
                );
                let size = [rect.width().max(1.0), rect.height().max(1.0)];
                if self.fit_requested || self.auto_frame {
                    self.camera.fit(&self.simulation.state, size[0] / size[1]);
                    self.fit_requested = false;
                }
                if ui.ctx().current_pass_index() == 0 {
                    if response.dragged() {
                        self.auto_frame = false;
                        let delta = response.drag_motion();
                        if response.dragged_by(egui::PointerButton::Secondary)
                            || ui.input(|i| i.modifiers.shift)
                        {
                            self.camera.pan([delta.x, delta.y], size[1]);
                        } else {
                            self.camera.orbit([delta.x, delta.y]);
                        }
                    }
                    if response.hovered() {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.auto_frame = false;
                            self.camera.distance =
                                (self.camera.distance * (-scroll * 0.002).exp()).clamp(0.001, 1e14);
                        }
                    }
                    if response.double_clicked() {
                        self.fit_requested = true;
                    }
                }
                ui.painter().add(EguiBevyPaintCallback::new_paint_callback(
                    rect,
                    Orbit3dCallback {
                        frame: self.frame.clone(),
                        uniform: self.camera.uniform(
                            size,
                            self.params.trail_opacity,
                            self.params.body_size,
                        ),
                        grid: self.params.grid,
                        trail_samples: self.params.trail_length,
                    },
                ));
                let status = if self.running { "RUNNING" } else { "PAUSED" };
                ui.painter().text(
                    rect.left_top() + egui::vec2(20.0, 20.0),
                    egui::Align2::LEFT_TOP,
                    format!("3D / {status}"),
                    egui::FontId::monospace(12.0),
                    Color32::from_rgb(107, 220, 207),
                );
                let warning = self.simulation.snapshot.error.as_deref().or_else(|| {
                    (self.simulation.snapshot.flags & 2 != 0)
                        .then_some("Work budget reached: simulation slowed to preserve accuracy.")
                });
                if let Some(warning) = warning {
                    let galley = ui.painter().layout(
                        warning.into(),
                        egui::FontId::proportional(13.0),
                        Color32::LIGHT_RED,
                        (rect.width() - 40.0).max(40.0),
                    );
                    ui.painter().galley(
                        rect.left_top() + egui::vec2(20.0, 45.0),
                        galley,
                        Color32::LIGHT_RED,
                    );
                }
            });
    }
}

impl Scene for ThreeBody3d {
    fn algorithm(&self) -> crate::integrators::Algorithm {
        self.params.algorithm
    }

    fn set_algorithm(
        &mut self,
        algorithm: crate::integrators::Algorithm,
    ) -> Result<(), &'static str> {
        self.params.set_algorithm(algorithm, "3d-threebody")
    }

    fn advance(&mut self, delta_seconds: f64) {
        if let Some(initial) = self.pending_reset.take() {
            self.simulation =
                Simulation::new(initial.clone(), self.simulation.snapshot.generation + 1)
                    .expect("validated initial state");
            self.active = initial;
            self.history.clear();
            self.history
                .push_back(self.simulation.state.bodies.map(|b| b.position));
            self.frame = Arc::new(Geometry::new(&self.simulation.state, &self.history));
            self.fit_requested = true;
        }
        let mut params = self.params.clone();
        params.duration = if std::mem::take(&mut self.step) {
            params.max_step
        } else if self.running {
            delta_seconds.clamp(0.0, 0.05) * f64::from(self.speed)
        } else {
            0.0
        };
        let before = self.simulation.snapshot.time;
        self.simulation.advance(&params);
        if self.simulation.snapshot.error.is_some() {
            self.running = false;
        }
        if self.simulation.snapshot.time > before {
            self.history
                .push_back(self.simulation.state.bodies.map(|b| b.position));
            if self.history.len() > HISTORY as usize {
                self.history.pop_front();
            }
            self.frame = Arc::new(Geometry::new(&self.simulation.state, &self.history));
        }
    }

    fn ui(&mut self, root: &mut egui::Ui) {
        if !self.initialized {
            self.show_controls = root.clip_rect().width() >= 760.0;
            self.initialized = true;
        }
        if root.ctx().current_pass_index() == 0 && !root.ctx().egui_wants_keyboard_input() {
            if root.input(|i| i.key_pressed(egui::Key::Space)) {
                self.running = !self.running;
            }
            if root.input(|i| i.key_pressed(egui::Key::R)) {
                self.restart(self.active.clone());
            }
        }
        egui::Panel::top("3d-header").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("THREE BODY / 3D").size(22.0).strong());
                if ui.button("Parameters").clicked() {
                    self.show_controls = !self.show_controls;
                }
                if ui
                    .button(if self.running { "Pause" } else { "Run" })
                    .clicked()
                {
                    self.running = !self.running;
                }
            });
        });
        if self.show_controls {
            self.controls(root);
        }
        egui::Panel::bottom("3d-diagnostics").show(root, |ui| {
            let s = &self.simulation.snapshot;
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("Time {:.3}", s.time));
                ui.monospace(format!("Drift {:.2e}", s.relative_energy_error()));
                ui.monospace(format!("Min r {:.3}", s.separation));
                ui.monospace(format!("Steps {}", s.steps));
            });
        });
        self.viewport(root);
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests;
