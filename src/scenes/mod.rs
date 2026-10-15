//! Application shell and scene lifecycle. Register new scenes in SCENES;
//! each factory owns its UI, simulation and display state independently.
mod mechanics;
mod style;
mod three_body;
mod three_d;
pub(crate) use style::configure_style;

use crate::integrators::{ALGORITHMS, Algorithm};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

pub trait Scene: Send + Sync {
    /// Called once per Bevy Update, never during egui layout or rendering.
    fn advance(&mut self, delta_seconds: f64);
    fn ui(&mut self, ui: &mut egui::Ui);
    fn is_running(&self) -> bool;
    fn algorithm(&self) -> Algorithm;
    /// Called at the Update boundary. Must preserve physical state and history.
    fn set_algorithm(&mut self, algorithm: Algorithm) -> Result<(), &'static str>;
}

pub struct SceneDefinition {
    /// Stable identifier, also displayed in the scene selector.
    pub id: &'static str,
    /// Ordered recommendations; the first entry is the default on scene entry.
    pub recommended_algorithms: &'static [Algorithm],
    pub create: fn() -> Box<dyn Scene>,
}

pub const SCENES: &[SceneDefinition] = &[
    SceneDefinition {
        id: "2d-345-threebody",
        recommended_algorithms: &[Algorithm::Dop853, Algorithm::Dopri5],
        create: three_body::create,
    },
    SceneDefinition {
        id: "3d-threebody",
        recommended_algorithms: &[Algorithm::Dop853, Algorithm::Dopri5],
        create: three_d::create,
    },
    SceneDefinition {
        id: "2d-double-pendulum",
        recommended_algorithms: &[Algorithm::Dop853, Algorithm::Dopri5],
        create: mechanics::double_pendulum,
    },
    SceneDefinition {
        id: "1d-coupled-springs",
        recommended_algorithms: &[Algorithm::Dop853, Algorithm::VelocityVerlet],
        create: mechanics::coupled_springs,
    },
    SceneDefinition {
        id: "2d-kepler-orbit",
        recommended_algorithms: &[Algorithm::Dop853, Algorithm::VelocityVerlet],
        create: mechanics::kepler_orbit,
    },
];

#[derive(Resource)]
pub struct SceneManager {
    definitions: &'static [SceneDefinition],
    active: usize,
    pending: Option<usize>,
    pending_algorithm: Option<Algorithm>,
    scene: Box<dyn Scene>,
    styled: bool,
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new(SCENES)
    }
}

impl SceneManager {
    fn new(definitions: &'static [SceneDefinition]) -> Self {
        assert!(!definitions.is_empty(), "register at least one scene");
        for (index, definition) in definitions.iter().enumerate() {
            assert!(!definition.id.is_empty(), "scene IDs must not be empty");
            assert!(
                !definitions[..index]
                    .iter()
                    .any(|other| other.id == definition.id),
                "scene IDs must be unique"
            );
        }
        let mut scene = (definitions[0].create)();
        if let Some(&algorithm) = definitions[0].recommended_algorithms.first() {
            scene
                .set_algorithm(algorithm)
                .expect("compatible scene default");
        }
        Self {
            definitions,
            active: 0,
            pending: None,
            pending_algorithm: None,
            scene,
            styled: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.scene.is_running()
    }

    fn advance(&mut self, delta_seconds: f64) {
        if let Some(next) = self.pending.take()
            && next != self.active
        {
            // Drop the old scene (including drafts, trails and pending commands).
            // Do not charge the new scene for time elapsed before it existed.
            self.scene = (self.definitions[next].create)();
            self.pending_algorithm = None;
            if let Some(&algorithm) = self.definitions[next].recommended_algorithms.first() {
                self.scene
                    .set_algorithm(algorithm)
                    .expect("compatible scene default");
            }
            self.active = next;
            self.scene.advance(0.0);
            return;
        }
        if let Some(algorithm) = self.pending_algorithm.take() {
            // Compatibility is checked again by the scene before changing params.
            let _ = self.scene.set_algorithm(algorithm);
        }
        self.scene.advance(delta_seconds);
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("scene-selector")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Scene");
                    let mut selected = self.pending.unwrap_or(self.active);
                    egui::ComboBox::from_id_salt("scene")
                        .selected_text(self.definitions[selected].id)
                        .show_ui(ui, |ui| {
                            for (index, definition) in self.definitions.iter().enumerate() {
                                ui.selectable_value(&mut selected, index, definition.id);
                            }
                        });
                    self.pending = (selected != self.active).then_some(selected);
                    ui.add_enabled_ui(self.pending.is_none(), |ui| {
                        let scene = &self.definitions[self.active];
                        let mut algorithm = self.pending_algorithm.unwrap_or(self.scene.algorithm());
                        ui.label("Algorithm");
                        egui::ComboBox::from_id_salt("algorithm")
                            .selected_text(algorithm.definition().name)
                            .show_ui(ui, |ui| {
                                for entry in ALGORITHMS.iter().filter(|entry| entry.algorithm.supports(scene.id)) {
                                    let label = if scene.recommended_algorithms.contains(&entry.algorithm) {
                                        format!("{} (recommended)", entry.name)
                                    } else { entry.name.into() };
                                    ui.selectable_value(&mut algorithm, entry.algorithm, label)
                                        .on_hover_text(format!("{}\nID: {}\nSupported scenes: {}", entry.description, entry.id, entry.supported_scenes.join(", ")));
                                }
                            });
                        self.pending_algorithm = (algorithm != self.scene.algorithm()).then_some(algorithm);
                        ui.label("About algorithms").on_hover_ui(|ui| {
                            ui.label(algorithm.definition().description);
                            ui.label(format!("Recommended: {}", scene.recommended_algorithms.iter().map(|a| a.definition().name).collect::<Vec<_>>().join(", ")));
                            ui.label(format!("Supported scenes: {}", algorithm.definition().supported_scenes.join(", ")));
                            ui.label("Switching preserves state, time and history; takes effect on the next update. Reset keeps the selected algorithm.");
                        });
                    });
                });
            });
        // Scope persistent egui widget state to the active scene as well.
        ui.push_id(self.definitions[self.active].id, |ui| self.scene.ui(ui));
    }
}

pub fn advance_scene(time: Res<Time<Real>>, mut scenes: ResMut<SceneManager>) {
    scenes.advance(time.delta_secs_f64());
}

pub fn draw_ui(mut contexts: EguiContexts, mut scenes: ResMut<SceneManager>) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !scenes.styled {
        configure_style(ctx);
        scenes.styled = true;
        #[cfg(target_arch = "wasm32")]
        if let Some(loading) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("loading"))
        {
            let _ = loading.set_attribute("style", "display:none");
        }
    }
    let mut root = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("laboratory"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    scenes.ui(&mut root);
    ctx.request_repaint_after(std::time::Duration::from_millis(if scenes.is_running() {
        16
    } else {
        100
    }));
    Ok(())
}

#[cfg(test)]
mod tests;
