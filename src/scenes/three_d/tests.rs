use super::*;

#[test]
fn pause_step_reset_errors_and_history_follow_scene_lifecycle() {
    let mut scene = ThreeBody3d::default();
    let frame = scene.frame.clone();
    scene.advance(60.0);
    assert_eq!(scene.simulation.snapshot.time, 0.0);
    assert!(Arc::ptr_eq(&frame, &scene.frame));
    scene.step = true;
    scene.advance(60.0);
    assert!((scene.simulation.snapshot.time - 0.05).abs() < 1e-14);
    assert_eq!(scene.history.len(), 2);
    assert!(!scene.step);
    scene.running = true;
    scene.advance(60.0);
    assert!((scene.simulation.snapshot.time - 0.1).abs() < 1e-14);
    scene.draft.bodies[0].velocity[2] += 0.5;
    let changed = scene.draft.clone();
    scene.restart(changed.clone());
    assert_eq!(scene.active, InitialConditions::default());
    assert!(scene.simulation.snapshot.time > 0.0);
    scene.advance(60.0);
    assert_eq!(scene.active, changed);
    assert_eq!(scene.simulation.snapshot.time, 0.0);
    assert_eq!(scene.history.len(), 1);
    assert!(!scene.running);
    assert_eq!(scene.frame.segments, 0);
    let mut invalid = changed.clone();
    invalid.bodies[0].velocity[2] = f64::INFINITY;
    scene.restart(invalid);
    assert!(scene.validation_error.is_some());
    assert!(scene.pending_reset.is_none());
    assert_eq!(scene.active, changed);
    scene.running = true;
    scene.params.rtol = f64::NAN;
    scene.advance(0.01);
    assert!(!scene.running);
    assert!(scene.simulation.snapshot.error.is_some());
    scene.params.rtol = 1e-11;
    scene.restart(scene.active.clone());
    scene.advance(0.01);
    assert!(scene.simulation.snapshot.error.is_none());
    // Fill the ring before advancing once; the oldest sample must be discarded.
    scene.history = VecDeque::from(vec![
        scene.active.bodies.map(|b| b.position);
        HISTORY as usize
    ]);
    scene.step = true;
    scene.advance(0.01);
    assert_eq!(scene.history.len(), HISTORY as usize);
    assert_eq!(scene.frame.segments, (HISTORY - 1) * 3);
    assert_eq!(frame.segments, 0);
}

#[test]
fn repeated_layout_only_toggles_shortcut_once_and_keeps_physics_unchanged() {
    let ctx = egui::Context::default();
    crate::scenes::configure_style(&ctx);
    let mut scene = ThreeBody3d::default();
    let frame = scene.frame.clone();
    let mut passes = 0;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(360.0, 800.0),
            )),
            events: vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            scene.ui(ui);
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("3D repeated layout");
            }
            assert!(ui.min_rect().width() <= 360.0);
        },
    );
    output.drop_without_applying_deltas();
    assert!(passes >= 2);
    assert!(scene.running);
    assert!(!scene.show_controls);
    assert_eq!(scene.simulation.snapshot.time, 0.0);
    assert!(Arc::ptr_eq(&frame, &scene.frame));
}

#[test]
fn orbit_camera_fits_bodies_at_narrow_and_wide_aspect_ratios() {
    use bevy::math::{Mat4, Vec3};
    for aspect in [0.25, 1.0, 2.0] {
        let mut camera = Camera::default();
        camera.orbit([4500.0, -10000.0]);
        let initial = InitialConditions::default();
        camera.fit(&initial, aspect);
        let bytes = camera.uniform([800.0 * aspect, 800.0], 1.0, 1.0);
        assert_eq!(bytes.len(), 128);
        let matrix = Mat4::from_cols_array(&std::array::from_fn(|i| {
            f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
        }));
        for body in initial.bodies {
            let p = matrix.project_point3(Vec3::from_array(body.position.map(|v| v as f32)));
            assert!(p.is_finite());
            assert!(p.x.abs() < 0.9 && p.y.abs() < 0.9 && (0.0..1.0).contains(&p.z));
        }
    }
}

#[test]
fn hot_swap_preserves_spatial_state_history_and_reset_selection() {
    let mut scene = ThreeBody3d::default();
    scene.running = true;
    scene.advance(0.05);
    scene.draft.bodies[0].position[2] += 0.1;
    for entry in crate::integrators::ALGORITHMS {
        let state = scene.simulation.state.clone();
        let time = scene.simulation.snapshot.time;
        let energy = scene.simulation.snapshot.initial_energy;
        let history = scene.history.clone();
        let draft = scene.draft.clone();
        scene.set_algorithm(entry.algorithm).unwrap();
        scene.advance(0.0);
        assert_eq!(scene.simulation.state, state);
        assert_eq!(scene.simulation.snapshot.time, time);
        assert_eq!(scene.simulation.snapshot.initial_energy, energy);
        assert_eq!(scene.history, history);
        assert_eq!(scene.draft, draft);
        assert!(scene.running);
        scene.advance(0.01);
        assert!(scene.simulation.snapshot.time > time);
        assert!(scene.simulation.snapshot.error.is_none());
    }
    let algorithm = scene.algorithm();
    scene.restart(scene.active.clone());
    scene.advance(0.0);
    assert_eq!(scene.algorithm(), algorithm);
    assert_eq!(scene.simulation.snapshot.time, 0.0);
}
