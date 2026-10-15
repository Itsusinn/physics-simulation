use super::*;

struct Oscillator;
impl System<f64, SVector<f64, 2>> for Oscillator {
    fn system(&self, _: f64, y: &SVector<f64, 2>, dy: &mut SVector<f64, 2>) {
        *dy = SVector::from([y[1], -y[0]]);
    }
}

fn oscillator(algorithm: Algorithm, step: f64, duration: f64, budget: u32) -> Output<2> {
    integrate(
        Oscillator,
        SVector::from([1.0, 0.0]),
        Request {
            params: &FrameParams {
                algorithm,
                max_step: step,
                ..Default::default()
            },
            start: 0.0,
            duration,
            next_step: 0.0,
            budget,
            position_dimensions: Some(1),
        },
    )
}

#[test]
fn every_backend_matches_analytic_oscillator() {
    for entry in ALGORITHMS {
        let out = oscillator(entry.algorithm, 0.001, 1.0, 2048);
        assert!(out.result.is_ok());
        let (time, state) = out.samples.last().unwrap();
        assert_eq!(*time, 1.0);
        let tolerance = if entry.algorithm == Algorithm::VelocityVerlet {
            2e-7
        } else {
            1e-10
        };
        assert!(
            (state - SVector::from([1.0_f64.cos(), -1.0_f64.sin()])).amax() < tolerance,
            "{}",
            entry.id
        );
    }
}

#[test]
fn fixed_methods_have_the_expected_convergence_order() {
    for (algorithm, expected_ratio) in [(Algorithm::Rk4, 16.0), (Algorithm::VelocityVerlet, 4.0)] {
        let error = |h| {
            let out = oscillator(algorithm, h, 1.0, 2048);
            (out.samples.last().unwrap().1 - SVector::from([1.0_f64.cos(), -1.0_f64.sin()])).norm()
        };
        let ratio = error(0.05) / error(0.025);
        assert!(
            (ratio - expected_ratio).abs() < 0.5,
            "{algorithm:?}: {ratio}"
        );
    }
}

#[test]
fn all_backends_bound_work_and_return_only_forward_samples() {
    for entry in ALGORITHMS {
        let out = oscillator(entry.algorithm, 0.001, 10.0, 8);
        assert!(matches!(
            out.result,
            Err(IntegrationError::MaxNumStepReached { .. })
        ));
        assert!(!out.samples.is_empty() && out.samples.len() <= 8);
        assert!(out.samples.windows(2).all(|pair| pair[1].0 > pair[0].0));
        assert!(out.samples.last().unwrap().0 < 0.02);
    }
}

#[test]
fn registry_references_real_scenes_and_recommendations_are_supported() {
    for (i, entry) in ALGORITHMS.iter().enumerate() {
        assert!(!entry.id.is_empty());
        assert!(
            !ALGORITHMS[..i]
                .iter()
                .any(|other| other.id == entry.id || other.algorithm == entry.algorithm)
        );
        for scene in entry.supported_scenes {
            assert!(crate::scenes::SCENES.iter().any(|s| s.id == *scene));
        }
    }
    for scene in crate::scenes::SCENES {
        assert!(!scene.recommended_algorithms.is_empty());
        for algorithm in scene.recommended_algorithms {
            assert!(algorithm.supports(scene.id));
        }
    }
    assert!(!Algorithm::VelocityVerlet.supports("2d-double-pendulum"));
}

#[test]
fn fixed_methods_detect_time_underflow_and_ignore_inactive_tolerances() {
    for algorithm in [Algorithm::Rk4, Algorithm::VelocityVerlet] {
        let params = FrameParams {
            algorithm,
            max_step: 0.01,
            rtol: f64::NAN,
            atol: 0.0,
            ..Default::default()
        };
        assert!(params.valid_integration_inputs(1.0));
        let out = integrate(
            Oscillator,
            SVector::from([1.0, 0.0]),
            Request {
                params: &params,
                start: 1e16,
                duration: 10.0,
                next_step: 0.0,
                budget: 10,
                position_dimensions: Some(1),
            },
        );
        assert!(matches!(
            out.result,
            Err(IntegrationError::StepSizeUnderflow { .. })
        ));
        assert!(out.samples.is_empty());
    }
}

#[test]
fn verlet_keeps_energy_bounded_over_many_periods() {
    let out = oscillator(Algorithm::VelocityVerlet, 0.02, 1000.0, 60_000);
    assert!(out.result.is_ok());
    let drift = out
        .samples
        .iter()
        .map(|(_, y)| (0.5 * y.norm_squared() - 0.5).abs())
        .fold(0.0_f64, f64::max);
    assert!(drift < 5.1e-5, "{drift}");
}
