use super::*;
use std::sync::Mutex;

static EVENTS: Mutex<Vec<(&str, f64)>> = Mutex::new(Vec::new());

struct ProbeScene;

impl Scene for ProbeScene {
    fn algorithm(&self) -> Option<Algorithm> {
        Some(Algorithm::Dop853)
    }
    fn set_algorithm(&mut self, _: Algorithm) -> Result<(), &'static str> {
        Ok(())
    }
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
        recommended_algorithms: &[Algorithm::Dop853],
        fixed_solver: None,
        create: three_body::create,
    },
    SceneDefinition {
        id: "test-only-scene",
        recommended_algorithms: &[],
        fixed_solver: None,
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

#[test]
fn registered_scenes_can_switch_both_directions() {
    let mut manager = SceneManager::default();
    assert_eq!(manager.definitions[0].id, "2d-345-threebody");
    assert_eq!(manager.definitions[1].id, "3d-threebody");
    let ctx = egui::Context::default();
    configure_style(&ctx);
    for index in (0..SCENES.len()).chain((0..SCENES.len()).rev()) {
        manager.pending = Some(index);
        manager.advance(60.0);
        assert_eq!(manager.active, index);
        assert!(!manager.is_running());
        let output = ctx.run_ui(egui::RawInput::default(), |ui| manager.ui(ui));
        assert!(!output.shapes.is_empty());
        output.drop_without_applying_deltas();
    }
}

#[test]
fn algorithm_switch_is_deferred_and_scene_switch_discards_pending_algorithm() {
    let mut manager = SceneManager::default();
    let ctx = egui::Context::default();
    configure_style(&ctx);
    manager.pending_algorithm = Some(Algorithm::Rk4);
    let mut passes = 0;
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        manager.ui(ui);
        assert_eq!(manager.scene.algorithm(), Some(Algorithm::Dop853));
        passes += 1;
        if passes == 1 {
            ui.ctx().request_discard("deferred algorithm selection");
        }
    });
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    manager.advance(0.0);
    assert_eq!(manager.scene.algorithm(), Some(Algorithm::Rk4));
    manager.pending_algorithm = Some(Algorithm::VelocityVerlet);
    manager.pending = Some(2);
    manager.advance(60.0);
    assert_eq!(manager.scene.algorithm(), Some(Algorithm::Dop853));
    assert!(manager.pending_algorithm.is_none());
    assert!(!manager.is_running());
    manager.pending_algorithm = Some(Algorithm::VelocityVerlet);
    manager.advance(0.0);
    assert_eq!(manager.scene.algorithm(), Some(Algorithm::Dop853));
}

#[test]
fn all_algorithm_labels_fit_narrow_layouts_and_do_not_change_during_layout() {
    for scene in 0..SCENES.len() {
        for entry in ALGORITHMS
            .iter()
            .filter(|entry| entry.algorithm.supports(SCENES[scene].id))
        {
            let mut manager = SceneManager::default();
            manager.pending = Some(scene);
            manager.advance(0.0);
            manager.pending_algorithm = Some(entry.algorithm);
            manager.advance(0.0);
            let ctx = egui::Context::default();
            configure_style(&ctx);
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    manager.ui(ui);
                    assert!(
                        ui.min_rect().width() <= 360.0,
                        "{} / {}",
                        SCENES[scene].id,
                        entry.id
                    );
                    assert_eq!(manager.scene.algorithm(), Some(entry.algorithm));
                },
            );
            output.drop_without_applying_deltas();
        }
    }
}
