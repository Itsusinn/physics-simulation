use super::*;

#[test]
fn pause_step_reset_drafts_budget_and_error_recovery() {
    let mut scene = Blocks::new();
    let initial = scene.simulation.snapshot;
    scene.advance(60.0);
    assert_eq!(scene.simulation.snapshot, initial);
    scene.step = true;
    scene.advance(60.0);
    assert_eq!(scene.simulation.snapshot.time, 0.05);
    assert!(!scene.running);
    scene.draft.masses = [0.25, 2500.5];
    scene.running = true;
    scene.advance(60.0);
    assert_eq!(scene.simulation.snapshot.time, 0.1);
    assert_eq!(scene.simulation.settings.masses, [1.0, 100.0]);
    // Long frames admit only 0.05 wall-clock seconds before Speed.
    scene.speed = 10.0;
    scene.advance(60.0);
    assert!(!scene.budget_limited);
    assert_eq!(scene.simulation.snapshot.time, 0.6);
    let before = scene.simulation.snapshot;
    scene.advance(0.0);
    assert_eq!(scene.simulation.snapshot, before, "no deferred catch-up");
    scene.restart(scene.draft.clone());
    assert_eq!(scene.simulation.snapshot, before, "reset is deferred");
    scene.advance(60.0);
    assert_eq!(scene.simulation.snapshot.time, 0.0);
    assert_eq!(scene.simulation.settings.masses, [0.25, 2500.5]);
    assert_eq!(scene.simulation.snapshot.collisions, [0; 2]);
    assert_eq!(scene.simulation.snapshot.total_collisions(), 0);
    assert!(!scene.running);
    scene.simulation.error = Some("test failure".into());
    scene.running = true;
    scene.advance(0.05);
    assert!(!scene.running);
    scene.restart(scene.simulation.settings.clone());
    scene.advance(0.05);
    assert!(scene.simulation.error.is_none());
    assert_eq!(scene.simulation.settings.masses, [0.25, 2500.5]);
    assert!(scene.set_algorithm(Algorithm::Dop853).is_err());
    assert_eq!(scene.algorithm(), None);
}

#[test]
fn invalid_drafts_are_isolated_and_free_flight_has_no_fixed_timestep() {
    let mut scene = Blocks::new();
    scene.running = true;
    scene.advance(0.0004);
    assert_eq!(scene.simulation.snapshot.time, 0.0004);
    scene.advance(0.0006);
    assert_eq!(scene.simulation.snapshot.time, 0.001);
    let before = scene.simulation.snapshot;
    scene.draft.restitution = f64::NAN;
    scene.restart(scene.draft.clone());
    assert!(scene.validation_error.is_some());
    assert!(scene.pending_reset.is_none());
    assert_eq!(scene.simulation.snapshot, before);
    assert!(scene.running);
    scene.advance(f64::INFINITY);
    assert_eq!(scene.simulation.snapshot, before);
    scene.draft.restitution = 1.0;
    for index in 0..2 {
        scene.draft.masses = [0.25, 2500.5];
        scene.draft.masses[index] = 0.0;
        scene.restart(scene.draft.clone());
        assert!(scene.validation_error.is_some());
        assert!(scene.pending_reset.is_none());
        assert_eq!(scene.simulation.snapshot, before);
        assert_eq!(scene.simulation.settings.masses, [1.0, 100.0]);
        assert!(scene.running);
    }
}

#[test]
fn analytic_impacts_give_identical_velocities_across_frame_partitions() {
    let mut a = Blocks::new();
    let mut b = Blocks::new();
    a.running = true;
    b.running = true;
    for _ in 0..250 {
        a.advance(0.02);
        b.advance(0.007);
        b.advance(0.013);
    }
    assert_eq!(
        a.simulation.snapshot.velocity,
        b.simulation.snapshot.velocity
    );
    assert_eq!(
        a.simulation.snapshot.collisions,
        b.simulation.snapshot.collisions
    );
    for (x, y) in a
        .simulation
        .snapshot
        .x
        .into_iter()
        .zip(b.simulation.snapshot.x)
    {
        assert!((x - y).abs() < 1e-11);
    }
    assert!(a.simulation.snapshot.collisions.iter().all(|&n| n > 0));
}

#[test]
fn dense_collision_burst_exposes_budget_without_skipping_events_or_catching_up() {
    let mut scene = Blocks::new();
    scene.restart(Settings {
        masses: [1.0, 1e8],
        ..Default::default()
    });
    scene.advance(0.0);
    scene.running = true;
    scene.speed = 10.0;
    for _ in 0..100 {
        scene.advance(0.05);
        assert!(scene.simulation.error.is_none());
        if scene.budget_limited {
            assert_eq!(scene.steps, MAX_IMPACTS);
            let before = scene.simulation.snapshot;
            scene.advance(0.0);
            assert_eq!(scene.simulation.snapshot, before);
            return;
        }
    }
    panic!("expected dense burst to exhaust the frame budget");
}

#[test]
fn narrow_wide_and_repeated_layout_do_not_advance_physics() {
    for width in [360.0, 1280.0] {
        for controls in [false, true] {
            let mut scene = Blocks::new();
            scene.running = true;
            scene.advance(0.05);
            scene.initialized = true;
            scene.show_controls = controls;
            let before = scene.simulation.snapshot;
            let ctx = egui::Context::default();
            super::super::configure_style(&ctx);
            let mut passes = 0;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(width, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    scene.ui(ui);
                    assert!(ui.min_rect().width() <= width);
                    passes += 1;
                    if passes == 1 {
                        ui.ctx().request_discard("verify multipass safety");
                    }
                },
            );
            assert!(passes >= 2);
            let meshes = ctx.tessellate(output.shapes.clone(), output.pixels_per_point);
            assert!(!meshes.is_empty());
            for mesh in meshes {
                if let egui::epaint::Primitive::Mesh(mesh) = mesh.primitive {
                    assert!(mesh.is_valid());
                    assert!(
                        mesh.vertices
                            .iter()
                            .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite())
                    );
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!(scene.simulation.snapshot, before);
        }
    }
}
