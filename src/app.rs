use std::{collections::VecDeque, sync::Arc};

use bevy::prelude::Resource;
use bevy_egui::render::EguiBevyPaintCallback;

use bevy_egui::egui::{self, Color32, FontId, RichText, Stroke, Vec2};

use crate::{
    gpu::OrbitCallback,
    model::{FrameParams, InitialConditions, Snapshot},
    runtime::DisplayFrame,
    simulation::Simulation,
};

const MUTED: Color32 = Color32::from_rgb(130, 151, 177);
const ACCENT: Color32 = Color32::from_rgb(107, 220, 207);
const COLORS: [Color32; 3] = [
    Color32::from_rgb(90, 197, 255),
    Color32::from_rgb(255, 175, 90),
    Color32::from_rgb(199, 153, 255),
];

#[derive(Resource)]
pub struct ThreeBodyApp {
    pub(crate) active: InitialConditions,
    draft: InitialConditions,
    pub(crate) params: FrameParams,
    snapshot: Snapshot,
    pub(crate) generation: u64,
    pub(crate) running: bool,
    pub(crate) step: bool,
    pub(crate) speed: f32,
    follow: bool,
    show_controls: bool,
    validation_error: Option<String>,
    energy_history: VecDeque<f64>,
    last_sample_time: f64,
    fps: f32,
    view_initialized: bool,
}

impl ThreeBodyApp {
    fn restart(&mut self, initial: InitialConditions) {
        if let Err(error) = Simulation::new(initial.clone(), self.generation + 1) {
            self.validation_error = Some(error.into());
            return;
        }
        self.active = initial.clone();
        self.generation += 1;
        self.snapshot = Snapshot::default();
        self.running = false;
        self.step = false;
        self.energy_history.clear();
        self.last_sample_time = -1.0;
        self.validation_error = None;
    }

    pub(crate) fn read_diagnostics(&mut self, latest: Snapshot) {
        if latest.generation != self.generation {
            return;
        }
        if latest.time > self.snapshot.time || latest.error.is_some() {
            self.step = false;
        }
        if latest.error.is_some() || latest.flags & 4 != 0 {
            self.running = false;
        }
        if latest.ready && latest.time != self.last_sample_time {
            self.energy_history
                .push_back(latest.relative_energy_error());
            if self.energy_history.len() > 160 {
                self.energy_history.pop_front();
            }
            self.last_sample_time = latest.time;
        }
        self.snapshot = latest;
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header")
            .exact_size(74.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 19, 30))
                    .inner_margin(egui::Margin::symmetric(22, 14)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("THREE BODY").size(22.0).strong());
                        ui.label(
                            RichText::new("3 : 4 : 5   /   ORBITAL LABORATORY")
                                .size(10.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Parameters").clicked() {
                            self.show_controls = !self.show_controls;
                        }
                        ui.label(RichText::new("WEBGPU  /  LIVE").size(11.0).color(ACCENT));
                    });
                });
            });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let panel_width = 320.0_f32.min((ui.available_width() - 10.0).max(200.0));
        egui::Panel::right("parameters").exact_size(panel_width).resizable(false)
            .frame(egui::Frame::new().fill(Color32::from_rgb(14, 22, 35)).inner_margin(18))
            .show(ui, |ui| {
                // Keep sliders inside the fixed panel, including their values and labels.
                ui.spacing_mut().slider_width = (ui.available_width() - 170.0).clamp(50.0, 115.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new("Simulation controls").size(18.0).strong());
                    ui.label(RichText::new("Explore a deterministic beginning.").small().color(MUTED));
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let text = if self.running { "Pause" } else { "Run" };
                        if ui.add_sized([86.0, 34.0], egui::Button::new(RichText::new(text).strong()).fill(Color32::from_rgb(31, 86, 85))).clicked() {
                            self.running = !self.running;
                        }
                        if ui.add_enabled(!self.running, egui::Button::new("Step")).clicked() { self.step = true; }
                        if ui.button("Reset").clicked() { self.restart(self.active.clone()); }
                    });
                    ui.label(RichText::new("Space: pause / run     R: reset").small().color(MUTED));
                    section(ui, "01", "Time & precision");
                    ui.add(egui::Slider::new(&mut self.speed, 0.05..=10.0).logarithmic(true).text("Speed"));
                    ui.add(egui::Slider::new(&mut self.params.max_step, 0.00001..=0.1).logarithmic(true).max_decimals(5).text("Max dt"));
                    ui.add(egui::Slider::new(&mut self.params.rtol, 1e-13..=1e-6).logarithmic(true).custom_formatter(|v, _| format!("{v:.0e}")).text("Rel. tol"));
                    ui.add(egui::Slider::new(&mut self.params.atol, 1e-15..=1e-8).logarithmic(true).custom_formatter(|v, _| format!("{v:.0e}")).text("Abs. tol"));
                    ui.label(RichText::new("Smaller tolerances request smaller local error; they do not guarantee global accuracy.").small().color(MUTED));
                    ui.label(RichText::new("DOP853 · f64 · adaptive error control").small().color(MUTED));
                    ui.label(RichText::new(format!("Smallest dt  {:.2e}", self.snapshot.min_step)).small().color(MUTED));

                    section(ui, "02", "Initial conditions");
                    ui.label(RichText::new("Changes below take effect on Apply.").small().color(MUTED));
                    ui.add(egui::Slider::new(&mut self.draft.gravity, 0.01..=10.0).logarithmic(true).text("Gravity G"));
                    ui.add(egui::Slider::new(&mut self.draft.softening, 0.0..=0.1).max_decimals(4).text("Softening"));
                    if self.draft.softening > 0.0 {
                        ui.label(RichText::new("Softening changes the Newtonian model.").small().color(Color32::from_rgb(230, 182, 109)));
                    }
                    for (i, color) in COLORS.iter().enumerate() {
                        egui::CollapsingHeader::new(RichText::new(format!("Body {}    /    m = {:.2}", i + 1, self.draft.bodies[i].mass)).color(*color))
                            .id_salt(i).default_open(i == 0).show(ui, |ui| {
                                let body = &mut self.draft.bodies[i];
                                ui.horizontal(|ui| {
                                    ui.label("Mass");
                                    ui.add(egui::DragValue::new(&mut body.mass).speed(0.05).range(0.01..=100.0));
                                });
                                egui::Grid::new(("body", i)).num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
                                    ui.label(""); ui.label("x"); ui.label("y"); ui.end_row();
                                    ui.label("Position");
                                    for p in &mut body.position { ui.add(egui::DragValue::new(p).speed(0.02).range(-100.0..=100.0).max_decimals(3)); }
                                    ui.end_row();
                                    ui.label("Velocity");
                                    for v in &mut body.velocity { ui.add(egui::DragValue::new(v).speed(0.01).range(-100.0..=100.0).max_decimals(3)); }
                                    ui.end_row();
                                });
                            });
                    }
                    let dirty = self.draft != self.active;
                    if ui.add_enabled(dirty, egui::Button::new("Apply & restart").min_size(Vec2::new(ui.available_width(), 32.0))).clicked() {
                        self.restart(self.draft.clone());
                    }
                    if ui.button("Restore classic 3-4-5").clicked() {
                        self.draft = InitialConditions::default();
                        self.restart(self.draft.clone());
                        self.fit_view(1.0);
                    }
                    if let Some(error) = &self.validation_error { ui.colored_label(Color32::LIGHT_RED, error); }

                    section(ui, "03", "View");
                    ui.add(egui::Slider::new(&mut self.params.trail_length, 32..=4096).text("Trail samples"));
                    ui.add(egui::Slider::new(&mut self.params.trail_opacity, 0.0..=1.0).text("Trail opacity"));
                    ui.add(egui::Slider::new(&mut self.params.body_size, 0.5..=2.0).text("Body size"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.params.grid, "Grid");
                        ui.checkbox(&mut self.follow, "Auto frame");
                    });
                    if ui.button("Fit all bodies").clicked() { self.fit_view(1.0); }
                    ui.label(RichText::new("Drag to pan · Scroll to zoom · Double-click to fit").small().color(MUTED));

                    section(ui, "04", "Conservation");
                    self.energy_plot(ui);
                    ui.label(RichText::new(format!("Energy   {:+.6}", self.snapshot.energy)).monospace().small());
                    ui.label(RichText::new(format!("|P|      {:.2e}", self.snapshot.momentum[0].hypot(self.snapshot.momentum[1]))).monospace().small());
                    ui.label(RichText::new(format!("Lz       {:+.2e}", self.snapshot.angular_momentum)).monospace().small());
                    ui.add_space(6.0);
                    ui.label(RichText::new("Chaotic trajectories amplify numerical errors. Watch energy drift when exploring close encounters.").small().color(MUTED));
                });
            });
    }

    fn fit_view(&mut self, weight: f32) {
        let positions = if self.snapshot.ready {
            self.snapshot.positions
        } else {
            self.active.bodies.map(|b| b.position)
        };
        let mut lo = [f32::INFINITY; 2];
        let mut hi = [f32::NEG_INFINITY; 2];
        for p in positions {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(p[axis] as f32);
                hi[axis] = hi[axis].max(p[axis] as f32);
            }
        }
        for axis in 0..2 {
            self.params.center[axis] +=
                ((lo[axis] + hi[axis]) * 0.5 - self.params.center[axis]) * weight;
        }
        let aspect = self.params.viewport[0] / self.params.viewport[1].max(1.0);
        let scale = ((hi[1] - lo[1]).max((hi[0] - lo[0]) / aspect.max(0.1)) * 0.7).clamp(1.0, 1e12);
        self.params.scale += (scale - self.params.scale) * weight;
    }

    fn energy_plot(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Relative energy drift  |E - E0| / |E0|")
                .small()
                .color(MUTED),
        );
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 5.0, Color32::from_rgb(8, 15, 25));
        let max = self
            .energy_history
            .iter()
            .copied()
            .fold(1e-14_f64, f64::max);
        let inner = rect.shrink(7.0);
        if self.energy_history.len() > 1 {
            let points: Vec<_> = self
                .energy_history
                .iter()
                .enumerate()
                .map(|(i, error)| {
                    egui::pos2(
                        inner.left()
                            + i as f32 / (self.energy_history.len() - 1) as f32 * inner.width(),
                        inner.bottom() - (error / max) as f32 * inner.height(),
                    )
                })
                .collect();
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(1.3, ACCENT)));
        }
        ui.painter().text(
            rect.left_top() + Vec2::new(7.0, 4.0),
            egui::Align2::LEFT_TOP,
            format!("{max:.1e}"),
            FontId::monospace(10.0),
            MUTED,
        );
    }

    fn footer(&self, ui: &mut egui::Ui) {
        let compact = ui.available_width() < 400.0;
        egui::Panel::bottom("diagnostics")
            .exact_size(if compact { 120.0 } else { 78.0 })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(12, 19, 30))
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ui, |ui| {
                ui.columns(if compact { 2 } else { 4 }, |columns| {
                    let readings = [
                        (
                            if compact { "TIME" } else { "SIMULATION TIME" },
                            format!("{:.3}", self.snapshot.time),
                        ),
                        (
                            if compact { "DRIFT" } else { "ENERGY DRIFT" },
                            format!("{:.2e}", self.snapshot.relative_energy_error()),
                        ),
                        (
                            if compact { "MIN r" } else { "MIN SEPARATION" },
                            format!("{:.4}", self.snapshot.separation),
                        ),
                        (
                            if compact { "STEPS" } else { "SUBSTEPS / FRAME" },
                            format!("{}", self.snapshot.steps),
                        ),
                    ];
                    let count = columns.len();
                    for (i, (label, value)) in readings.into_iter().enumerate() {
                        let column = &mut columns[i % count];
                        column.label(RichText::new(label).size(9.0).color(MUTED));
                        column.label(
                            RichText::new(value)
                                .size(if compact { 13.0 } else { 20.0 })
                                .monospace(),
                        );
                    }
                });
            });
    }

    fn viewport(&mut self, ui: &mut egui::Ui, frame: Arc<DisplayFrame>) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let (rect, response) = ui.allocate_exact_size(
                    ui.available_size().max(Vec2::splat(1.0)),
                    egui::Sense::click_and_drag(),
                );
                self.params.viewport = [rect.width().max(1.0), rect.height().max(1.0)];
                if !self.view_initialized {
                    self.fit_view(1.0);
                    self.view_initialized = true;
                }
                if response.dragged() {
                    self.follow = false;
                    let delta =
                        response.drag_motion() * (2.0 * self.params.scale / rect.height().max(1.0));
                    self.params.center[0] -= delta.x;
                    self.params.center[1] += delta.y;
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.follow = false;
                        self.params.scale =
                            (self.params.scale * (-scroll * 0.002).exp()).clamp(0.01, 1e12);
                    }
                }
                if response.double_clicked() {
                    self.fit_view(1.0);
                }
                if self.follow {
                    self.fit_view(0.12);
                }
                ui.painter().add(EguiBevyPaintCallback::new_paint_callback(
                    rect,
                    OrbitCallback {
                        params: self.params.clone(),
                        frame,
                    },
                ));
                let painter = ui.painter();
                let status = if self.running { "RUNNING" } else { "PAUSED" };
                painter.text(
                    rect.left_top() + Vec2::new(24.0, 24.0),
                    egui::Align2::LEFT_TOP,
                    if rect.width() < 400.0 {
                        status.to_owned()
                    } else {
                        format!("PYTHAGOREAN SYSTEM    /    {status}")
                    },
                    FontId::monospace(11.0),
                    ACCENT,
                );
                painter.text(
                    rect.left_top() + Vec2::new(24.0, 48.0),
                    egui::Align2::LEFT_TOP,
                    if rect.width() < 400.0 {
                        "3 : 4 : 5"
                    } else {
                        "Three masses. One chaotic system."
                    },
                    FontId::proportional(17.0),
                    Color32::from_rgb(210, 224, 240),
                );
                for (i, color) in COLORS.iter().enumerate() {
                    let p = rect.left_bottom()
                        + if rect.width() < 400.0 {
                            Vec2::new(25.0, -76.0 + i as f32 * 24.0)
                        } else {
                            Vec2::new(25.0 + i as f32 * 120.0, -28.0)
                        };
                    painter.circle_filled(p + Vec2::new(0.0, 6.0), 3.5, *color);
                    painter.text(
                        p + Vec2::new(12.0, 0.0),
                        egui::Align2::LEFT_TOP,
                        format!("m{}  {:.2}", i + 1, self.active.bodies[i].mass),
                        FontId::monospace(12.0),
                        *color,
                    );
                }
                painter.text(
                    rect.right_top() + Vec2::new(-20.0, 24.0),
                    egui::Align2::RIGHT_TOP,
                    if rect.width() < 400.0 {
                        String::new()
                    } else {
                        format!("{:.0} FPS", self.fps)
                    },
                    FontId::monospace(11.0),
                    MUTED,
                );
                let warning = if let Some(error) = &self.snapshot.error {
                    Some(format!("Integration stopped: {error}"))
                } else if self.snapshot.flags & 4 != 0 {
                    Some("Precision limit reached. Reset the simulation.".into())
                } else if self.snapshot.flags & 2 != 0 {
                    Some("Work budget reached: simulation slowed to preserve accuracy.".into())
                } else if self.snapshot.relative_energy_error() > 0.01 {
                    Some("Energy drift exceeds 1%. Tighten tolerances and restart.".into())
                } else {
                    None
                };
                if let Some(warning) = warning {
                    let color = Color32::from_rgb(255, 185, 115);
                    let text = painter.layout(
                        warning,
                        FontId::proportional(12.0),
                        color,
                        (rect.width() - 48.0).max(40.0),
                    );
                    painter.galley(rect.left_top() + Vec2::new(24.0, 80.0), text, color);
                }
            });
    }
}

impl ThreeBodyApp {
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, frame: Arc<DisplayFrame>) {
        if !self.view_initialized && ui.available_width() < 760.0 {
            self.show_controls = false;
        }
        let dt = ui.input(|i| i.unstable_dt).max(0.001);
        self.fps = self.fps * 0.95 + 0.05 / dt;
        if ui.ctx().current_pass_index() == 0 && !ui.ctx().egui_wants_keyboard_input() {
            if ui.input(|i| i.key_pressed(egui::Key::Space)) {
                self.running = !self.running;
            }
            if ui.input(|i| i.key_pressed(egui::Key::R)) {
                self.restart(self.active.clone());
            }
        }
        self.header(ui);
        if self.show_controls {
            self.controls(ui);
        }
        self.footer(ui);
        self.viewport(ui, frame);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(if self.running {
                16
            } else {
                100
            }));
    }
}

fn section(ui: &mut egui::Ui, number: &str, title: &str) {
    ui.add_space(9.0);
    ui.separator();
    ui.add_space(3.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(number).monospace().small().color(ACCENT));
        ui.label(RichText::new(title).strong());
    });
}

impl Default for ThreeBodyApp {
    fn default() -> Self {
        Self {
            active: InitialConditions::default(),
            draft: InitialConditions::default(),
            params: FrameParams::default(),
            snapshot: Snapshot::default(),
            generation: 0,
            running: false,
            step: false,
            speed: 1.0,
            follow: false,
            show_controls: true,
            validation_error: None,
            energy_history: VecDeque::new(),
            last_sample_time: -1.0,
            fps: 60.0,
            view_initialized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Laboratory;

    #[test]
    fn repeated_egui_layout_does_not_toggle_shortcuts_twice_or_advance_physics() {
        let ctx = egui::Context::default();
        crate::scenes::configure_style(&ctx);
        let mut ui_state = ThreeBodyApp::default();
        let lab = Laboratory::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1000.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut passes = 0;
        let output = ctx.run_ui(input, |ui| {
            ui_state.ui(ui, lab.frame.clone());
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("exercise repeated layout");
            }
        });
        assert!(passes >= 2);
        assert!(ui_state.running);
        assert_eq!(lab.simulation.snapshot.time, 0.0);
        assert_eq!(lab.frame.revision, 1);
        assert!(!output.shapes.is_empty());
        output.drop_without_applying_deltas();
    }

    #[test]
    fn restart_rejects_invalid_state_without_replacing_active_conditions() {
        let mut app = ThreeBodyApp::default();
        let original = app.active.clone();
        let mut invalid = original.clone();
        invalid.bodies[0].position[0] = f64::MAX;
        app.restart(invalid);
        assert!(app.validation_error.is_some());
        assert_eq!(app.generation, 0);
        assert_eq!(app.active, original);
    }
}
