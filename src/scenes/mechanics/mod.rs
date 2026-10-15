//! Independent classic mechanics scenes using the existing solver and egui GPU renderer.
mod physics;

use std::collections::VecDeque;

use bevy_egui::egui::{self, Color32, Pos2, Stroke};

use super::Scene;
use crate::model::{FrameParams, HISTORY};
use physics::{Kind, Model, Simulation, State};

const COLORS: [Color32; 2] = [
    Color32::from_rgb(104, 218, 207),
    Color32::from_rgb(255, 182, 105),
];

pub(super) fn double_pendulum() -> Box<dyn Scene> {
    Box::new(Mechanics::new(Kind::DoublePendulum))
}
pub(super) fn coupled_springs() -> Box<dyn Scene> {
    Box::new(Mechanics::new(Kind::CoupledSprings))
}
pub(super) fn kepler_orbit() -> Box<dyn Scene> {
    Box::new(Mechanics::new(Kind::KeplerOrbit))
}

struct Mechanics {
    simulation: Simulation,
    draft: Model,
    pending_reset: Option<Model>,
    validation_error: Option<String>,
    params: FrameParams,
    history: VecDeque<(f64, State)>,
    running: bool,
    step: bool,
    speed: f64,
    show_controls: bool,
    initialized: bool,
    auto_frame: bool,
    fit_requested: bool,
    center: [f64; 2],
    span: f64,
}

impl Mechanics {
    fn new(kind: Kind) -> Self {
        let draft = Model::preset(kind, 0);
        Self {
            simulation: Simulation::new(draft.clone()).expect("valid mechanics preset"),
            history: VecDeque::from([(0.0, draft.initial)]),
            draft,
            pending_reset: None,
            validation_error: None,
            params: FrameParams::default(),
            running: false,
            step: false,
            speed: 1.0,
            show_controls: true,
            initialized: false,
            auto_frame: true,
            fit_requested: true,
            center: [0.0; 2],
            span: 6.0,
        }
    }

    fn restart(&mut self, model: Model) {
        match model.validate() {
            Ok(()) => {
                self.pending_reset = Some(model);
                self.running = false;
                self.step = false;
                self.validation_error = None;
            }
            Err(error) => self.validation_error = Some(error.into()),
        }
    }

    fn controls(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("mechanics-parameters")
            .exact_size(300.0_f32.min((root.available_width() - 12.0).max(180.0)))
            .resizable(false).show(root, |ui| {
                ui.spacing_mut().slider_width = (ui.available_width() - 150.0).clamp(40.0, 110.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let kind = self.draft.kind;
                    ui.heading("Experiment");
                    ui.label(kind.description());
                    ui.small("Consistent arbitrary units; no dissipation.");
                    ui.separator();
                    ui.add(egui::Slider::new(&mut self.speed, 0.05..=10.0).logarithmic(true).text("Speed"));
                    ui.add(egui::Slider::new(&mut self.params.max_step, 0.00001..=0.1).logarithmic(true).text("Max dt"));
                    crate::integrators::precision_controls(ui, &mut self.params);
                    ui.separator();
                    ui.strong("Presets");
                    for (index, name) in kind.presets().iter().enumerate() {
                        if ui.button(*name).clicked() {
                            self.draft = Model::preset(kind, index);
                            self.restart(self.draft.clone());
                        }
                    }
                    ui.separator();
                    ui.strong("Initial conditions");
                    ui.small("Edits take effect on Apply & restart.");
                    egui::Grid::new("mechanics-inputs").num_columns(2).show(ui, |ui| {
                        for (index, name) in kind.parameter_names().iter().enumerate() {
                            ui.label(*name);
                            let minimum = if kind == Kind::CoupledSprings && index == 3 { 0.0 } else { 0.01 };
                            ui.add(egui::DragValue::new(&mut self.draft.parameters[index]).speed(0.01).range(minimum..=100.0));
                            ui.end_row();
                        }
                        for (index, name) in kind.state_names().iter().enumerate() {
                            ui.label(*name);
                            ui.add(egui::DragValue::new(&mut self.draft.initial[index]).speed(0.01).range(-100.0..=100.0));
                            ui.end_row();
                        }
                    });
                    if ui.add_enabled(self.draft != self.simulation.model, egui::Button::new("Apply & restart")).clicked() {
                        self.restart(self.draft.clone());
                    }
                    if let Some(error) = &self.validation_error { ui.colored_label(Color32::LIGHT_RED, error); }
                    ui.separator();
                    ui.strong("View");
                    ui.checkbox(&mut self.auto_frame, "Auto frame");
                    ui.checkbox(&mut self.params.grid, "Grid");
                    ui.add(egui::Slider::new(&mut self.params.trail_length, 32..=HISTORY).text("History samples"));
                    if ui.button("Fit experiment").clicked() { self.fit_requested = true; }
                    ui.small("Drag: pan / Scroll: zoom / Double-click: fit");
                    ui.separator();
                    let s = &self.simulation;
                    let [kinetic, potential] = s.model.energy_parts(&s.state);
                    ui.strong("Diagnostics");
                    ui.monospace(format!("Kinetic   {kinetic:+.6}"));
                    ui.monospace(format!("Potential {potential:+.6}"));
                    ui.monospace(format!("Energy    {:+.6}", kinetic + potential));
                    ui.small("Scaled drift = (E - E0) / (|K0| + |U0|), with a 1e-15 denominator floor.");
                    match kind {
                        Kind::KeplerOrbit => {
                            let h = s.state[0] * s.state[3] - s.state[1] * s.state[2];
                            let e = (1.0 + 2.0 * (kinetic + potential) * h * h / s.model.parameters[0].powi(2)).max(0.0).sqrt();
                            ui.monospace(format!("Angular L {h:+.6}"));
                            ui.monospace(format!("Eccentricity {e:.6}"));
                            ui.small("Integration stops near r = 1e-6; this is a numerical boundary, not a collision model.");
                        }
                        Kind::DoublePendulum => { ui.small("Potential energy is zero at the hanging equilibrium. Chaotic motion amplifies small initial and numerical differences."); }
                        Kind::CoupledSprings => {
                            let [m1, m2, k, coupling, _] = s.model.parameters;
                            let e1 = 0.5 * (m1 * s.state[2].powi(2) + k * s.state[0].powi(2));
                            let e2 = 0.5 * (m2 * s.state[3].powi(2) + k * s.state[1].powi(2));
                            ui.monospace(format!("Oscillator 1 {e1:.6}"));
                            ui.monospace(format!("Oscillator 2 {e2:.6}"));
                            ui.monospace(format!("Coupling     {:.6}", 0.5 * coupling * (s.state[0] - s.state[1]).powi(2)));
                            ui.small("Bottom chart: displacement versus actual simulation time. Mass markers and spring spacing are schematic.");
                        }
                    }
                });
            });
    }

    fn fit(&mut self, aspect: f64) {
        let model = &self.simulation.model;
        let mut low = [0.0_f64; 2];
        let mut high = [0.0_f64; 2];
        match model.kind {
            Kind::DoublePendulum => {
                let reach = model.parameters[2] + model.parameters[3];
                low = [-reach; 2];
                high = [reach; 2];
            }
            Kind::CoupledSprings => {
                low = [-5.0, -2.0];
                high = [5.0, 2.0];
            }
            Kind::KeplerOrbit => {}
        }
        for (_, y) in self
            .history
            .iter()
            .rev()
            .take(self.params.trail_length as usize)
        {
            for position in model.positions(y) {
                for axis in 0..2 {
                    low[axis] = low[axis].min(position[axis]);
                    high[axis] = high[axis].max(position[axis]);
                }
            }
        }
        self.center = std::array::from_fn(|i| 0.5 * (low[i] + high[i]));
        self.span = ((high[1] - low[1]).max((high[0] - low[0]) / aspect) * 1.25).max(1.0);
    }

    fn viewport(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(root, |ui| {
                let (rect, response) = ui.allocate_exact_size(
                    ui.available_size().max(egui::Vec2::splat(1.0)),
                    egui::Sense::click_and_drag(),
                );
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 19, 29));
                let kind = self.simulation.model.kind;
                let mut world_rect = rect.shrink(18.0);
                if kind == Kind::CoupledSprings {
                    world_rect.max.y -= rect.height() * 0.3;
                }
                if world_rect.width() < 2.0 || world_rect.height() < 2.0 {
                    return;
                }
                if self.auto_frame || self.fit_requested {
                    self.fit(f64::from(world_rect.width() / world_rect.height()));
                    self.fit_requested = false;
                }
                if ui.ctx().current_pass_index() == 0 {
                    if response.dragged() {
                        self.auto_frame = false;
                        let delta = response.drag_motion();
                        self.center[0] -= f64::from(delta.x / world_rect.height()) * self.span;
                        self.center[1] += f64::from(delta.y / world_rect.height()) * self.span;
                    }
                    if response.hovered() {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll != 0.0 {
                            self.auto_frame = false;
                            self.span =
                                (self.span * f64::from(-scroll * 0.002).exp()).clamp(1e-4, 1e14);
                        }
                    }
                    if response.double_clicked() {
                        self.fit_requested = true;
                    }
                }
                // Physical coordinates remain f64 until this final display projection.
                let project = |p: [f64; 2]| -> Pos2 {
                    world_rect.center()
                        + egui::vec2(
                            ((p[0] - self.center[0]) / self.span) as f32 * world_rect.height(),
                            -((p[1] - self.center[1]) / self.span) as f32 * world_rect.height(),
                        )
                };
                let world = painter.with_clip_rect(world_rect);
                if self.params.grid {
                    let spacing = 10.0_f64.powf((self.span / 6.0).log10().floor());
                    let half_width =
                        self.span * f64::from(world_rect.width() / world_rect.height()) * 0.5;
                    for axis in 0..2 {
                        let extent = if axis == 0 {
                            half_width
                        } else {
                            self.span * 0.5
                        };
                        let first = ((self.center[axis] - extent) / spacing).floor();
                        let count = (2.0 * extent / spacing).ceil().min(200.0) as usize + 2;
                        for i in 0..count {
                            let coordinate = (first + i as f64) * spacing;
                            let (a, b) = if axis == 0 {
                                (
                                    [coordinate, self.center[1] - self.span],
                                    [coordinate, self.center[1] + self.span],
                                )
                            } else {
                                (
                                    [self.center[0] - half_width, coordinate],
                                    [self.center[0] + half_width, coordinate],
                                )
                            };
                            world.line_segment(
                                [project(a), project(b)],
                                Stroke::new(1.0, Color32::from_rgb(28, 42, 55)),
                            );
                        }
                    }
                }
                let model = &self.simulation.model;
                if kind != Kind::CoupledSprings {
                    for (body, color) in COLORS.iter().enumerate() {
                        if kind == Kind::KeplerOrbit && body == 0 {
                            continue;
                        }
                        let points: Vec<_> = self
                            .history
                            .iter()
                            .rev()
                            .take(self.params.trail_length as usize)
                            .map(|(_, y)| project(model.positions(y)[body]))
                            .collect();
                        world.add(egui::Shape::line(
                            points,
                            Stroke::new(1.5, color.gamma_multiply(0.5)),
                        ));
                    }
                }
                let points = model.positions(&self.simulation.state).map(project);
                match kind {
                    Kind::DoublePendulum => {
                        world.line_segment(
                            [project([0.0, 0.0]), points[0]],
                            Stroke::new(3.0, Color32::GRAY),
                        );
                        world.line_segment(points, Stroke::new(3.0, Color32::GRAY));
                        world.circle_filled(project([0.0, 0.0]), 5.0, Color32::WHITE);
                    }
                    Kind::CoupledSprings => {
                        let left = project([-5.0, 0.0]);
                        let right = project([5.0, 0.0]);
                        for pair in [[left, points[0]], points, [points[1], right]] {
                            spring(&world, pair[0], pair[1]);
                        }
                        for anchor in [left, right] {
                            world.line_segment(
                                [
                                    anchor - egui::vec2(0.0, 18.0),
                                    anchor + egui::vec2(0.0, 18.0),
                                ],
                                Stroke::new(4.0, Color32::GRAY),
                            );
                        }
                        self.chart(
                            &painter,
                            egui::Rect::from_min_max(
                                egui::pos2(rect.left() + 24.0, world_rect.bottom() + 28.0),
                                rect.right_bottom() - egui::vec2(24.0, 24.0),
                            ),
                        );
                    }
                    Kind::KeplerOrbit => {}
                }
                for (index, point) in points.into_iter().enumerate() {
                    let radius = if kind == Kind::KeplerOrbit && index == 0 {
                        13.0
                    } else {
                        9.0
                    };
                    world.circle_filled(point, radius + 5.0, COLORS[index].gamma_multiply(0.12));
                    world.circle_filled(point, radius, COLORS[index]);
                }
                painter.text(
                    rect.left_top() + egui::vec2(18.0, 16.0),
                    egui::Align2::LEFT_TOP,
                    if self.running { "RUNNING" } else { "PAUSED" },
                    egui::FontId::monospace(12.0),
                    COLORS[0],
                );
            });
    }

    fn chart(&self, painter: &egui::Painter, rect: egui::Rect) {
        if rect.width() < 4.0 || rect.height() < 4.0 {
            return;
        }
        let samples: Vec<_> = self
            .history
            .iter()
            .rev()
            .take(self.params.trail_length as usize)
            .rev()
            .collect();
        let start = samples.first().map_or(0.0, |s| s.0);
        let end = self.simulation.time.max(start + 0.01);
        let amplitude = samples
            .iter()
            .flat_map(|(_, y)| [y[0].abs(), y[1].abs()])
            .fold(0.1_f64, f64::max)
            * 1.15;
        painter.line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1.0, Color32::DARK_GRAY),
        );
        for (i, color) in COLORS.iter().enumerate() {
            let points = samples
                .iter()
                .map(|(time, y)| {
                    egui::pos2(
                        rect.left() + ((time - start) / (end - start)) as f32 * rect.width(),
                        rect.center().y - (y[i] / amplitude) as f32 * rect.height() * 0.5,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(1.5, *color)));
        }
        painter.text(
            rect.left_top(),
            egui::Align2::LEFT_BOTTOM,
            format!("Displacement 1 / 2    {start:.1} .. {end:.1} s"),
            egui::FontId::monospace(11.0),
            Color32::LIGHT_GRAY,
        );
    }
}

fn spring(painter: &egui::Painter, start: Pos2, end: Pos2) {
    let mut points = vec![start];
    for i in 1..20 {
        let mut p = start.lerp(end, i as f32 / 20.0);
        p.y += if i % 2 == 0 { -7.0 } else { 7.0 };
        points.push(p);
    }
    points.push(end);
    painter.add(egui::Shape::line(points, Stroke::new(2.0, Color32::GRAY)));
}

impl Scene for Mechanics {
    fn algorithm(&self) -> Option<crate::integrators::Algorithm> {
        Some(self.params.algorithm)
    }

    fn set_algorithm(
        &mut self,
        algorithm: crate::integrators::Algorithm,
    ) -> Result<(), &'static str> {
        self.params
            .set_algorithm(algorithm, self.simulation.model.kind.scene_id())
    }

    fn advance(&mut self, delta_seconds: f64) {
        if let Some(model) = self.pending_reset.take() {
            self.simulation = Simulation::new(model).expect("validated pending reset");
            self.history = VecDeque::from([(0.0, self.simulation.state)]);
            self.fit_requested = true;
            return;
        }
        let duration = if std::mem::take(&mut self.step) {
            self.params.max_step
        } else if self.running && delta_seconds.is_finite() {
            delta_seconds.clamp(0.0, 0.05) * self.speed
        } else {
            0.0
        };
        let before = self.simulation.time;
        self.simulation.advance(duration, &self.params);
        if self.simulation.error.is_some() {
            self.running = false;
        }
        if self.simulation.time > before {
            self.history
                .push_back((self.simulation.time, self.simulation.state));
            if self.history.len() > HISTORY as usize {
                self.history.pop_front();
            }
        }
    }

    fn ui(&mut self, root: &mut egui::Ui) {
        if !self.initialized {
            self.show_controls = root.clip_rect().width() >= 760.0;
            self.initialized = true;
        }
        if root.ctx().current_pass_index() == 0 && !root.ctx().egui_wants_keyboard_input() {
            if root.input(|i| i.key_pressed(egui::Key::Space)) && self.simulation.error.is_none() {
                self.running = !self.running;
            }
            if root.input(|i| i.key_pressed(egui::Key::R)) {
                self.restart(self.simulation.model.clone());
            }
        }
        egui::Panel::top("mechanics-header").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(self.simulation.model.kind.title());
                if ui.button("Parameters").clicked() {
                    self.show_controls = !self.show_controls;
                }
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
                    self.restart(self.simulation.model.clone());
                }
            });
        });
        // Reserve full-width diagnostics before allocating the side panel.
        egui::Panel::bottom("mechanics-status").show(root, |ui| {
            let s = &self.simulation;
            ui.horizontal_wrapped(|ui| {
                ui.monospace(format!("Time {:.3}", s.time));
                ui.monospace(format!("Energy {:+.6}", s.model.energy(&s.state)));
                ui.monospace(format!(
                    "Scaled drift {:+.2e}",
                    (s.model.energy(&s.state) - s.initial_energy) / s.energy_scale
                ));
                ui.monospace(format!("Steps {}", s.steps));
            });
            if let Some(error) = &s.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            } else if s.limited {
                ui.colored_label(
                    Color32::YELLOW,
                    "Work budget reached: simulation slowed to preserve accuracy.",
                );
            }
        });
        if self.show_controls {
            self.controls(root);
        }
        self.viewport(root);
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests;
