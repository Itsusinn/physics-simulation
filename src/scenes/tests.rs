use super::*;
use std::sync::Mutex;

static EVENTS: Mutex<Vec<(&str, f64)>> = Mutex::new(Vec::new());

struct ProbeScene;

impl Scene for ProbeScene {
    fn advance(&mut self, delta_seconds: f64) {
        EVENTS.lock().unwrap().push(("advance", delta_seconds));
    }
    fn ui(&mut self, _ui: &mut egui::Ui) {}
    fn is_running(&self) -> bool {
        true
    }
}

impl Drop for ProbeScene {
    fn drop(&mut self) {
        EVENTS.lock().unwrap().push(("drop", 0.0));
    }
}

fn probe() -> Box<dyn Scene> {
    EVENTS.lock().unwrap().push(("create", 0.0));
    Box::new(ProbeScene)
}

static TEST_SCENES: &[SceneDefinition] = &[
    SceneDefinition {
        id: "2d-345-threebody",
        create: three_body::create,
    },
    SceneDefinition {
        id: "test-only-scene",
        create: probe,
    },
];

#[test]
fn switching_is_deferred_drops_old_scene_and_reenters_paused() {
    EVENTS.lock().unwrap().clear();
    let mut app = App::new();
    app.init_resource::<Time<Real>>()
        .insert_resource(SceneManager::new(TEST_SCENES))
        .add_systems(Update, advance_scene);
    let ctx = egui::Context::default();
    configure_style(&ctx);
    let mut passes = 0;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut manager = app.world_mut().resource_mut::<SceneManager>();
        manager.pending = Some(1);
        manager.ui(ui);
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("test repeated scene layout");
        }
    });
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(EVENTS.lock().unwrap().is_empty());
    assert_eq!(app.world().resource::<SceneManager>().active, 0);

    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_secs(60));
    app.update();
    assert_eq!(*EVENTS.lock().unwrap(), [("create", 0.0), ("advance", 0.0)]);
    assert!(app.world().resource::<SceneManager>().is_running());

    // Selecting the current scene does not recreate it or erase its state.
    app.world_mut().resource_mut::<SceneManager>().pending = Some(1);
    app.update();
    assert_eq!(EVENTS.lock().unwrap().last(), Some(&("advance", 60.0)));
    assert_eq!(
        EVENTS
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| *event == "create")
            .count(),
        1
    );

    app.world_mut().resource_mut::<SceneManager>().pending = Some(0);
    app.update();
    let manager = app.world().resource::<SceneManager>();
    assert_eq!(manager.definitions[manager.active].id, "2d-345-threebody");
    assert!(!manager.is_running());
    assert_eq!(EVENTS.lock().unwrap().last(), Some(&("drop", 0.0)));
}

#[test]
fn selector_and_scene_fit_narrow_and_wide_layouts() {
    for width in [360.0, 1280.0] {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut manager = SceneManager::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 800.0),
                )),
                ..Default::default()
            },
            |ui| {
                manager.ui(ui);
                assert!(
                    ui.min_rect().width() <= width,
                    "scene shell must not overflow"
                );
            },
        );
        assert!(!output.shapes.is_empty());
        assert!(!manager.is_running());
        assert_eq!(manager.pending, None);
        output.drop_without_applying_deltas();
    }
}
