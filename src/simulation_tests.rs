use super::*;

fn run(
    initial: InitialConditions,
    end: f64,
    rtol: f64,
    atol: f64,
    frame: f64,
) -> (Simulation, f64) {
    let mut simulation = Simulation::new(initial, 0).unwrap();
    let mut worst = 0.0_f64;
    while simulation.snapshot.time < end {
        simulation.advance(&FrameParams {
            duration: frame.min(end - simulation.snapshot.time),
            rtol,
            atol,
            ..Default::default()
        });
        assert!(
            simulation.snapshot.error.is_none(),
            "{:?}",
            simulation.snapshot.error
        );
        assert!(simulation.snapshot.steps > 0);
        worst = worst.max(simulation.snapshot.relative_energy_error());
    }
    (simulation, worst)
}

#[test]
fn smooth_infall_matches_independent_rk4() {
    let initial = InitialConditions::default();
    let reference = reference_rk4(&initial, 0.0001, 10_000);
    let (simulation, _) = run(initial, 1.0, 1e-11, 1e-13, 0.05);
    for (body, expected) in simulation.state.bodies.iter().zip(reference) {
        for (actual, reference) in body.position.into_iter().chain(body.velocity).zip(expected) {
            assert!(
                (actual - reference).abs() < 1e-10,
                "{actual} != {reference}"
            );
        }
    }
    assert!((simulation.snapshot.time - 1.0).abs() < 1e-14);
    assert!(simulation.snapshot.relative_energy_error() < 1e-11);
}

#[test]
fn close_encounters_converge_when_tolerances_are_tightened() {
    let (normal, drift) = run(InitialConditions::default(), 10.0, 1e-11, 1e-13, 0.05);
    let (tight, tight_drift) = run(InitialConditions::default(), 10.0, 1e-13, 1e-15, 0.05);
    let (split, _) = run(InitialConditions::default(), 10.0, 1e-13, 1e-15, 0.017);
    let difference = |a: &Simulation, b: &Simulation| (vector(&a.state) - vector(&b.state)).amax();
    eprintln!(
        "t=10: drift={drift:.3e}, tight drift={tight_drift:.3e}, tolerance difference={:.3e}, frame difference={:.3e}",
        difference(&normal, &tight),
        difference(&tight, &split)
    );
    assert!(drift < 1e-8);
    assert!(tight_drift < 1e-9);
    assert!(difference(&normal, &tight) < 1e-5);
    assert!(difference(&tight, &split) < 1e-6);
    assert!(normal.snapshot.momentum[0].hypot(normal.snapshot.momentum[1]) < 1e-10);
    assert!(normal.snapshot.angular_momentum.abs() < 1e-9);
}

#[test]
fn lagrange_equilateral_solution_matches_analytic_orbit() {
    let mut initial = InitialConditions::default();
    let omega = (1.0 / 3.0_f64.sqrt()).sqrt();
    for (i, body) in initial.bodies.iter_mut().enumerate() {
        let angle = i as f64 * std::f64::consts::TAU / 3.0;
        body.mass = 1.0;
        body.position = [angle.cos(), angle.sin()];
        body.velocity = [-omega * angle.sin(), omega * angle.cos()];
    }
    let end = 2.0;
    let (simulation, drift) = run(initial, end, 1e-11, 1e-13, 0.05);
    for (i, body) in simulation.state.bodies.iter().enumerate() {
        let angle = i as f64 * std::f64::consts::TAU / 3.0 + omega * end;
        assert!((body.position[0] - angle.cos()).abs() < 1e-11);
        assert!((body.position[1] - angle.sin()).abs() < 1e-11);
    }
    assert!(drift < 1e-11);
}

#[test]
fn pause_step_reset_budget_and_softened_coincidence() {
    let initial = InitialConditions::default();
    let mut simulation = Simulation::new(initial.clone(), 7).unwrap();
    assert!((simulation.snapshot.energy + 769.0 / 60.0).abs() < 1e-14);
    simulation.advance(&FrameParams::default());
    assert_eq!(simulation.state, initial);
    assert_eq!(
        simulation.snapshot.positions,
        initial.bodies.map(|b| b.position)
    );
    assert_eq!(simulation.snapshot.time, 0.0);
    assert_eq!(simulation.snapshot.steps, 0);
    let params = FrameParams {
        duration: 0.05,
        ..Default::default()
    };
    simulation.advance(&params);
    assert_eq!(simulation.snapshot.time, params.max_step);
    let advanced = simulation.state.clone();
    simulation.advance(&FrameParams::default());
    assert_eq!(simulation.state, advanced);
    let mut reset = Simulation::new(initial.clone(), 8).unwrap();
    assert_eq!(reset.snapshot.generation, 8);
    reset.advance(&FrameParams {
        duration: 0.5,
        max_step: 1e-5,
        ..Default::default()
    });
    assert_eq!(reset.snapshot.flags & 2, 2);
    assert_eq!(reset.snapshot.steps, MAX_ATTEMPTS);
    assert!(reset.snapshot.time > 0.0 && reset.snapshot.time < 0.021);
    assert!(reset.snapshot.error.is_none());
    reset.advance(&FrameParams {
        duration: 0.001,
        ..Default::default()
    });
    assert_eq!(reset.snapshot.flags & 2, 0);
    let mut softened = initial;
    softened.bodies[1].position = softened.bodies[0].position;
    assert!(Simulation::new(softened.clone(), 0).is_err());
    softened.softening = 0.1;
    let (simulation, drift) = run(softened, 0.01, 1e-11, 1e-13, 0.001);
    assert!(simulation.snapshot.energy.is_finite());
    assert!(drift < 1e-8);
}

#[test]
fn precision_and_invalid_input_failures_preserve_the_last_valid_state() {
    let initial = InitialConditions::default();
    let mut simulation = Simulation::new(initial.clone(), 0).unwrap();
    simulation.snapshot.time = 1e20;
    simulation.advance(&FrameParams {
        duration: 0.01,
        ..Default::default()
    });
    assert!(simulation.snapshot.error.is_some());
    assert_eq!(simulation.state, initial);
    simulation.advance(&FrameParams::default());
    assert!(simulation.snapshot.error.is_some());
    let mut simulation = Simulation::new(initial.clone(), 0).unwrap();
    simulation.advance(&FrameParams {
        duration: 0.1,
        rtol: f64::NAN,
        ..Default::default()
    });
    assert!(simulation.snapshot.error.is_some());
    assert_eq!(simulation.state, initial);
    let mut extreme = initial;
    extreme.bodies[0].velocity[0] = 1e300;
    assert!(Simulation::new(extreme, 0).is_err());

    // Finite initial forces can still exceed the solver's error-scaling range.
    // Exercise the actual solver failure path, including underflow at t=0.
    let mut close = InitialConditions::default();
    for (i, body) in close.bodies.iter_mut().enumerate() {
        body.position = [i as f64 * 1e-100, 0.0];
    }
    let mut simulation = Simulation::new(close.clone(), 0).unwrap();
    simulation.advance(&FrameParams {
        duration: 0.01,
        ..Default::default()
    });
    assert!(simulation.snapshot.error.is_some());
    assert_eq!(simulation.snapshot.flags & 4, 4);
    assert_eq!(simulation.snapshot.time, 0.0);
    assert_eq!(simulation.state, close);
}

// Independent fixed-step RK4 reference for the smooth initial infall.
fn reference_rk4(initial: &InitialConditions, dt: f64, steps: usize) -> [[f64; 4]; 3] {
    let mut y = initial
        .bodies
        .map(|b| [b.position[0], b.position[1], b.velocity[0], b.velocity[1]]);
    let derivative = |y: [[f64; 4]; 3]| {
        let mut d = [[0.0; 4]; 3];
        for i in 0..3 {
            d[i][0] = y[i][2];
            d[i][1] = y[i][3];
            for j in 0..3 {
                if i == j {
                    continue;
                }
                let dx = y[j][0] - y[i][0];
                let dy = y[j][1] - y[i][1];
                let radius2 = dx * dx + dy * dy + initial.softening.powi(2);
                let force = (initial.gravity * initial.bodies[j].mass) / radius2.powf(1.5);
                d[i][2] += dx * force;
                d[i][3] += dy * force;
            }
        }
        d
    };
    let add = |a: [[f64; 4]; 3], b: [[f64; 4]; 3], factor: f64| {
        std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] + b[i][j] * factor))
    };
    for _ in 0..steps {
        let k1 = derivative(y);
        let k2 = derivative(add(y, k1, dt * 0.5));
        let k3 = derivative(add(y, k2, dt * 0.5));
        let k4 = derivative(add(y, k3, dt));
        y = std::array::from_fn(|i| {
            std::array::from_fn(|j| {
                y[i][j] + dt / 6.0 * (k1[i][j] + 2.0 * k2[i][j] + 2.0 * k3[i][j] + k4[i][j])
            })
        });
    }
    y
}
