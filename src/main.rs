mod app;
mod gpu;
mod model;
mod runtime;
mod scenes;
mod simulation;

use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{RenderCreation, WgpuSettings},
    },
    winit::{UpdateMode, WinitSettings},
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

fn main() {
    let mut settings = WgpuSettings::default();
    #[cfg(target_arch = "wasm32")]
    {
        settings.backends = Some(wgpu::Backends::BROWSER_WEBGPU);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        settings.backends = Some(wgpu::Backends::PRIMARY);
    }
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Three Body - Orbital laboratory".into(),
                        canvas: Some("#simulation".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        resolution: (1280, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(Box::new(settings)),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .init_resource::<scenes::SceneManager>()
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera2d, Msaa::Off));
        })
        .add_systems(Update, scenes::advance_scene)
        .add_systems(EguiPrimaryContextPass, scenes::draw_ui)
        .add_systems(
            PostUpdate,
            update_loop_mode.after(bevy_egui::EguiPostUpdateSet::EndPass),
        )
        .run();
}

fn update_loop_mode(scenes: Res<scenes::SceneManager>, mut settings: ResMut<WinitSettings>) {
    let interval = std::time::Duration::from_millis(if scenes.is_running() { 16 } else { 100 });
    let mode = UpdateMode::reactive_low_power(interval);
    settings.focused_mode = mode;
    settings.unfocused_mode = mode;
}
