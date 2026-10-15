use super::*;

const KINDS: [Kind; 3] = [
    Kind::DoublePendulum,
    Kind::CoupledSprings,
    Kind::KeplerOrbit,
];

#[test]
fn pause_step_reset_drafts_long_frames_and_history_capacity() {
    for kind in KINDS {
        let mut scene = Mechanics::new(kind);
        scene.advance(60.0);
        assert_eq!(scene.simulation.time, 0.0);
        assert_eq!(scene.history.len(), 1);
        scene.step = true;
        scene.advance(60.0);
        assert!((scene.simulation.time - 0.05).abs() < 1e-14);
        assert!(!scene.running);
        assert_eq!(scene.history.len(), 2);
        scene.draft.initial[0] += 0.1;
        assert_ne!(scene.draft, scene.simulation.model);
        scene.running = true;
        scene.advance(60.0);
        assert!((scene.simulation.time - 0.1).abs() < 1e-14);
        for _ in 0..HISTORY + 2 {
            scene.advance(0.001);
        }
        assert_eq!(scene.history.len(), HISTORY as usize);
        let applied = scene.draft.clone();
        scene.restart(applied.clone());
        assert!(scene.simulation.time > 0.0);
        scene.advance(60.0);
        assert_eq!(scene.simulation.time, 0.0);
        assert_eq!(scene.simulation.model, applied);
        assert_eq!(scene.history.len(), 1);
        assert!(!scene.running);
        scene.simulation.error = Some("test numerical failure".into());
        scene.running = true;
        scene.advance(0.05);
        assert!(!scene.running);
        scene.restart(scene.simulation.model.clone());
        scene.advance(0.05);
        assert!(scene.simulation.error.is_none());
    }
}

#[test]
fn invalid_draft_does_not_reset_or_modify_running_experiment() {
    let mut scene = Mechanics::new(Kind::KeplerOrbit);
    scene.running = true;
    scene.advance(0.05);
    let before = scene.simulation.state;
    scene.draft.initial.fill(0.0);
    scene.restart(scene.draft.clone());
    assert!(scene.validation_error.is_some());
    assert!(scene.pending_reset.is_none());
    assert!(scene.running);
    assert_eq!(before, scene.simulation.state);
}

#[test]
fn ui_tessellates_at_narrow_and_wide_sizes_without_advancing_physics() {
    for kind in KINDS {
        for width in [360.0, 1280.0] {
            for controls in [false, true] {
                let mut scene = Mechanics::new(kind);
                scene.running = true;
                scene.advance(0.05);
                scene.initialized = true;
                scene.show_controls = controls;
                let before = scene.simulation.state;
                let time = scene.simulation.time;
                let count = scene.history.len();
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
                        assert!(
                            ui.min_rect().width() <= width,
                            "{kind:?} width {width} controls {controls}"
                        );
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
                assert_eq!(before, scene.simulation.state);
                assert_eq!(time, scene.simulation.time);
                assert_eq!(count, scene.history.len());
            }
        }
    }
}

#[test]
fn hot_swap_preserves_state_history_draft_and_running_status() {
    for kind in KINDS {
        let mut scene = Mechanics::new(kind);
        scene.running = true;
        scene.advance(0.05);
        scene.draft.initial[0] += 0.1;
        for entry in crate::integrators::ALGORITHMS {
            let state = scene.simulation.state;
            let time = scene.simulation.time;
            let history = scene.history.clone();
            let draft = scene.draft.clone();
            let energy = scene.simulation.initial_energy;
            let before_algorithm = scene.algorithm();
            let result = scene.set_algorithm(entry.algorithm);
            if !entry.algorithm.supports(kind.scene_id()) {
                assert!(result.is_err());
                assert_eq!(scene.algorithm(), before_algorithm);
                continue;
            }
            result.unwrap();
            scene.advance(0.0);
            assert_eq!(scene.simulation.state, state);
            assert_eq!(scene.simulation.time, time);
            assert_eq!(scene.history, history);
            assert_eq!(scene.draft, draft);
            assert_eq!(scene.simulation.initial_energy, energy);
            assert!(scene.running);
            scene.advance(0.01);
            assert!(scene.simulation.time > time);
            assert!(scene.simulation.error.is_none());
            scene.running = false;
            scene.set_algorithm(entry.algorithm).unwrap();
            scene.advance(60.0);
            assert_eq!(scene.history.len(), history.len() + 1);
            scene.running = true;
        }
        let algorithm = scene.algorithm();
        scene.restart(scene.simulation.model.clone());
        scene.advance(0.0);
        assert_eq!(scene.algorithm(), algorithm);
        assert_eq!(scene.simulation.time, 0.0);
        scene.step = true;
        scene.advance(0.0);
        assert!(scene.simulation.time > 0.0);
        assert!(scene.simulation.error.is_none());
    }
}
