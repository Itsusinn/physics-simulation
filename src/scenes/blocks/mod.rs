//! A scene-owned Avian world, advanced only by the application's Update.
mod physics;

use super::Scene;
use crate::integrators::Algorithm;
use bevy_egui::egui::{self, Color32, Pos2, Stroke};
use physics::{DT, SIZES, Settings, Simulation};

pub(super) const SOLVER: super::FixedSolver = super::FixedSolver {
    name: "Avian 2D · collision solver",
    description: "Fixed 0.001 s timestep. Resolves rigid-body contacts; ODE integrators are unavailable in this scene.",
    supported_scenes: &["2d-block-collisions"],
};

const MAX_STEPS: usize = 256;
const COLORS: [Color32; 2] = [
    Color32::from_rgb(104, 218, 207),
    Color32::from_rgb(255, 182, 105),
];

pub(super) fn create() -> Box<dyn Scene> {
    Box::new(Blocks::new())
}

struct Blocks {
    simulation: Simulation,
    draft: Settings,
    pending_reset: Option<Settings>,
    validation_error: Option<String>,
    running: bool,
    step: bool,
    speed: f64,
    remainder: f64,
    steps: usize,
    budget_limited: bool,
    show_controls: bool,
    initialized: bool,
    auto_frame: bool,
    fit: bool,
    center: [f64; 2],
    span: f64,
}

impl Blocks {
    fn new() -> Self {
        Self {
            simulation: Simulation::new(Settings::default()).expect("valid block preset"),
            draft: Settings::default(),
            pending_reset: None,
            validation_error: None,
            running: false,
            step: false,
            speed: 1.0,
            remainder: 0.0,
            steps: 0,
            budget_limited: false,
            show_controls: true,
            initialized: false,
            auto_frame: true,
            fit: true,
            center: [3.0, 1.0],
            span: 8.0,
        }
    }

    fn restart(&mut self, settings: Settings) {
        match settings.validate() {
            Ok(()) => {
                self.pending_reset = Some(settings);
                self.running = false;
                self.step = false;
                self.validation_error = None;
            }
            Err(error) => self.validation_error = Some(error.into()),
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sliding blocks");
        ui.label("A heavy block hits a stationary light block. The light block bounces between it and the wall.");
        ui.small("Horizontal motion only. No friction or gravity. Consistent arbitrary units.");
        ui.separator();
        ui.spacing_mut().slider_width = (ui.available_width() - 150.0).clamp(40.0, 110.0);
        ui.add(
            egui::Slider::new(&mut self.speed, 0.05..=10.0)
                .logarithmic(true)
                .text("Speed"),
        );
        ui.small("Fixed physics step: 0.001 s");
        ui.small("Avian: f64 positions; f32 velocities and contact solver.");
        ui.separator();
        ui.strong("Initial conditions");
        ui.small("Edits take effect on Apply & restart.");
        egui::Grid::new("block-inputs")
            .num_columns(2)
            .show(ui, |ui| {
                for (index, label) in ["Small mass", "Large mass"].iter().enumerate() {
                    ui.label(*label);
                    ui.add(egui::DragValue::new(&mut self.draft.masses[index]).range(1..=100));
                    ui.end_row();
                }
                ui.label("Incoming speed");
                ui.add(
                    egui::DragValue::new(&mut self.draft.incoming_speed)
                        .range(0.1..=3.0)
                        .speed(0.01),
                );
                ui.end_row();
                ui.label("Restitution");
                ui.add(
                    egui::DragValue::new(&mut self.draft.restitution)
                        .range(0.0..=1.0)
                        .speed(0.01),
                );
                ui.end_row();
            });
        ui.small("Restitution 1: elastic; 0: no rebound. Small block starts at rest, large block moves left.");
        if ui
            .add_enabled(
                self.draft != self.simulation.settings,
                egui::Button::new("Apply & restart"),
            )
            .clicked()
        {
            self.restart(self.draft.clone());
        }
        if ui.button("Restore 1 : 100 preset").clicked() {
            self.draft = Settings::default();
            self.restart(self.draft.clone());
        }
        if let Some(error) = &self.validation_error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        ui.separator();
        ui.checkbox(&mut self.auto_frame, "Auto frame");
        if ui.button("Fit experiment").clicked() {
            self.fit = true;
        }
        ui.small("Drag: pan / Scroll: zoom / Double-click: fit");
        ui.separator();
        let s = self.simulation.snapshot;
        ui.strong("Diagnostics");
        ui.strong(format!("Total collisions  {}", s.total_collisions()));
        ui.monospace(format!("Block / block  {}", s.collisions[0]));
        ui.monospace(format!("Small / wall   {}", s.collisions[1]));
        ui.monospace(format!("Small vx  {:+.6}", s.velocity[0]));
        ui.monospace(format!("Large vx  {:+.6}", s.velocity[1]));
        ui.monospace(format!("Kinetic   {:.6}", s.kinetic));
        ui.monospace(format!(
            "ΔK / K₀   {:+.3e}",
            (s.kinetic - self.simulation.initial_energy) / self.simulation.initial_energy
        ));
        ui.small("Counts mark new contacts. Kinetic energy can decrease when restitution < 1. Numerical drift is shown even for elastic collisions.");
    }

    fn view(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(
            ui.available_size().max(egui::vec2(1.0, 1.0)),
            egui::Sense::drag(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 19, 29));
        let aspect = f64::from(rect.width() / rect.height().max(1.0));
        if self.fit || self.auto_frame {
            let right = (self.simulation.snapshot.x[1] + SIZES[1] * 0.5).max(6.0);
            self.center = [(right - 0.3) * 0.5, 1.5];
            self.span = ((right + 0.8) * 1.2).max(4.5 * aspect);
            self.fit = false;
        }
        if response.dragged() {
            let delta = ui.input(|i| i.pointer.delta());
            let scale = f64::from(rect.width()) / self.span;
            self.center[0] -= f64::from(delta.x) / scale;
            self.center[1] += f64::from(delta.y) / scale;
            self.auto_frame = false;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.span = (self.span * (-f64::from(scroll) * 0.002).exp()).clamp(1.0, 1e13);
                self.auto_frame = false;
            }
        }
        if response.double_clicked() {
            self.fit = true;
        }
        let scale = f64::from(rect.width()) / self.span;
        let project = |x: f64, y: f64| {
            Pos2::new(
                rect.center().x + ((x - self.center[0]) * scale) as f32,
                rect.center().y - ((y - self.center[1]) * scale) as f32,
            )
        };
        let wall_color = Color32::from_rgb(105, 126, 148);
        // The horizontal leg continues to the viewport edge, like a rail.
        let rail_end = (self.center[0] + self.span * 0.5).max(6.5);
        for (min, max) in [([-0.3, 0.0], [0.0, 3.4]), ([-0.3, -0.3], [rail_end, 0.0])] {
            painter.rect_filled(
                egui::Rect::from_two_pos(project(min[0], min[1]), project(max[0], max[1])),
                0.0,
                wall_color,
            );
        }
        for i in 0..2 {
            let x = self.simulation.snapshot.x[i];
            let bounds = egui::Rect::from_two_pos(
                project(x - SIZES[i] / 2.0, 0.0),
                project(x + SIZES[i] / 2.0, SIZES[i]),
            );
            painter.rect_filled(bounds, 3.0, COLORS[i].gamma_multiply(0.8));
            painter.rect_stroke(
                bounds,
                3.0,
                Stroke::new(1.5, COLORS[i]),
                egui::StrokeKind::Inside,
            );
            painter.text(
                bounds.center(),
                egui::Align2::CENTER_CENTER,
                format!("m = {}", self.simulation.settings.masses[i]),
                egui::FontId::proportional(14.0),
                Color32::from_rgb(12, 19, 29),
            );
            let velocity = self.simulation.snapshot.velocity[i];
            if velocity.abs() > 1e-4 {
                let length = (velocity.abs() * 24.0).clamp(16.0, 80.0) as f32;
                painter.arrow(
                    project(x, SIZES[i] + 0.25),
                    egui::vec2(length * velocity.signum() as f32, 0.0),
                    Stroke::new(2.0, COLORS[i]),
                );
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(16.0, 16.0),
            egui::Align2::LEFT_TOP,
            "SLIDING BLOCKS",
            egui::FontId::proportional(16.0),
            Color32::from_rgb(185, 202, 218),
        );
        painter.text(
            rect.left_top() + egui::vec2(16.0, 42.0),
            egui::Align2::LEFT_TOP,
            "Frictionless rail · fixed wall",
            egui::FontId::proportional(12.0),
            Color32::GRAY,
        );
        // Keep the total visible in the experiment even with Parameters hidden.
        // On narrow views, place the counter below the title instead of beside it.
        if rect.width() >= 240.0 && rect.height() >= 180.0 {
            let origin = if rect.width() >= 560.0 {
                rect.right_top() + egui::vec2(-232.0, 12.0)
            } else {
                rect.left_top() + egui::vec2(16.0, 66.0)
            };
            let badge = egui::Rect::from_min_size(origin, egui::vec2(216.0, 94.0));
            painter.rect_filled(badge, 8.0, Color32::from_rgb(18, 30, 43));
            let s = self.simulation.snapshot;
            for (offset, text, size, color) in [
                (
                    12.0,
                    "TOTAL COLLISIONS".to_owned(),
                    12.0,
                    Color32::LIGHT_GRAY,
                ),
                (30.0, s.total_collisions().to_string(), 30.0, COLORS[0]),
                (
                    72.0,
                    format!("Blocks: {}   Wall: {}", s.collisions[0], s.collisions[1]),
                    12.0,
                    Color32::LIGHT_GRAY,
                ),
            ] {
                painter.text(
                    origin + egui::vec2(12.0, offset),
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::proportional(size),
                    color,
                );
            }
        }
    }
}

impl Scene for Blocks {
    fn algorithm(&self) -> Option<Algorithm> {
        None
    }
    fn set_algorithm(&mut self, _: Algorithm) -> Result<(), &'static str> {
        Err("This scene uses Avian's collision solver.")
    }
    fn is_running(&self) -> bool {
        self.running
    }

    fn advance(&mut self, delta_seconds: f64) {
        self.steps = 0;
        self.budget_limited = false;
        if let Some(settings) = self.pending_reset.take() {
            self.simulation = Simulation::new(settings).expect("validated block settings");
            self.running = false;
            self.step = false;
            self.remainder = 0.0;
            self.fit = true;
            return;
        }
        if self.simulation.error.is_some() {
            self.running = false;
            self.step = false;
            return;
        }
        let single_step = std::mem::take(&mut self.step);
        let duration = if single_step && !self.running {
            0.05
        } else if self.running && delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, 0.05) * self.speed
        } else {
            return;
        };
        self.remainder += duration;
        let requested = ((self.remainder + DT * 1e-9) / DT).floor() as usize;
        self.budget_limited = requested > MAX_STEPS;
        self.remainder = (self.remainder - requested as f64 * DT).max(0.0);
        for _ in 0..requested.min(MAX_STEPS) {
            self.simulation.tick();
            if self.simulation.error.is_some() {
                self.running = false;
                break;
            }
            self.steps += 1;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        if !self.initialized {
            self.show_controls = ui.available_width() >= 760.0;
            self.initialized = true;
        }
        if ui.ctx().current_pass_index() == 0 && !ui.ctx().egui_wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(egui::Key::Space)) && self.simulation.error.is_none() {
                self.running = !self.running;
            }
            if ui.input(|i| i.key_pressed(egui::Key::R)) {
                self.restart(self.simulation.settings.clone());
            }
        }
        egui::Panel::top("block-actions").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.simulation.error.is_none(),
                        egui::Button::new(if self.running { "Pause" } else { "Run" }),
                    )
                    .clicked()
                {
                    self.running = !self.running;
                }
                if ui
                    .add_enabled(
                        !self.running && self.simulation.error.is_none(),
                        egui::Button::new("Step"),
                    )
                    .clicked()
                {
                    self.step = true;
                }
                if ui.button("Reset").clicked() {
                    self.restart(self.simulation.settings.clone());
                }
                ui.toggle_value(&mut self.show_controls, "Parameters");
                ui.label(format!("t = {:.3}", self.simulation.snapshot.time));
                ui.label(format!(
                    "Total collisions: {}",
                    self.simulation.snapshot.total_collisions()
                ));
            });
        });
        egui::Panel::bottom("block-status").show(ui, |ui| {
            if let Some(error) = &self.simulation.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if self.budget_limited {
                ui.colored_label(
                    Color32::YELLOW,
                    "Physics budget reached; simulation slowed down.",
                );
            } else {
                ui.label(format!(
                    "{} · {} physics steps · Space: run / pause · R: reset",
                    if self.running { "Running" } else { "Paused" },
                    self.steps
                ));
            }
        });
        if self.show_controls {
            egui::Panel::right("block-parameters")
                .exact_size(300.0_f32.min((ui.available_width() - 12.0).max(180.0)))
                .resizable(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.controls(ui));
                });
        }
        self.view(ui);
    }
}

#[cfg(test)]
mod tests;
