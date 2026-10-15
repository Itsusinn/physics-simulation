//! Application shell and scene lifecycle. Register new scenes in SCENES;
//! each factory owns its UI, simulation and display state independently.
mod mechanics;
mod style;
mod three_body;
mod three_d;
pub(crate) use style::configure_style;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

pub trait Scene: Send + Sync {
    /// Called once per Bevy Update, never during egui layout or rendering.
    fn advance(&mut self, delta_seconds: f64);
    fn ui(&mut self, ui: &mut egui::Ui);
    fn is_running(&self) -> bool;
}

pub struct SceneDefinition {
    /// Stable identifier, also displayed in the scene selector.
    pub id: &'static str,
    pub create: fn() -> Box<dyn Scene>,
}

pub const SCENES: &[SceneDefinition] = &[
    SceneDefinition {
        id: "2d-345-threebody",
        create: three_body::create,
    },
    SceneDefinition {
        id: "3d-threebody",
        create: three_d::create,
    },
    SceneDefinition {
        id: "2d-double-pendulum",
        create: mechanics::double_pendulum,
    },
    SceneDefinition {
        id: "1d-coupled-springs",
        create: mechanics::coupled_springs,
    },
    SceneDefinition {
        id: "2d-kepler-orbit",
        create: mechanics::kepler_orbit,
    },
];

#[derive(Resource)]
pub struct SceneManager {
    definitions: &'static [SceneDefinition],
    active: usize,
    pending: Option<usize>,
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
        Self {
            definitions,
            active: 0,
            pending: None,
            scene: (definitions[0].create)(),
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
            self.active = next;
            self.scene.advance(0.0);
            return;
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
