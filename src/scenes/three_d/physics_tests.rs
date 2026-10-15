use super::*;
use crate::scenes::three_d::model::Body;

fn run(initial: InitialConditions, end: f64, interval: f64, tolerance: f64) -> Simulation {
    let mut sim = Simulation::new(initial, 0).unwrap();
    while sim.snapshot.time < end {
        sim.advance(&FrameParams {
            duration: (end - sim.snapshot.time).min(interval),
            rtol: tolerance,
            atol: tolerance * 0.01,
            ..Default::default()
        });
        assert!(sim.snapshot.error.is_none(), "{:?}", sim.snapshot.error);
    }
    sim
}

#[test]
fn planar_limit_matches_existing_2d_solver() {
    let mut planar =
        crate::simulation::Simulation::new(crate::model::InitialConditions::default(), 0).unwrap();
    for _ in 0..20 {
        planar.advance(&FrameParams {
            duration: 0.05,
            ..Default::default()
        });
    }
    let spatial = run(InitialConditions::classic(), 1.0, 0.05, 1e-11);
    for (a, b) in spatial.state.bodies.iter().zip(planar.state.bodies) {
        for axis in 0..2 {
            assert!((a.position[axis] - b.position[axis]).abs() < 1e-9);
            assert!((a.velocity[axis] - b.velocity[axis]).abs() < 1e-9);
        }
        assert_eq!(a.position[2], 0.0);
        assert_eq!(a.velocity[2], 0.0);
    }
}

// Independent fixed-step RK4 reference with direct body-by-body acceleration.
fn rk4(initial: &InitialConditions, duration: f64, steps: usize) -> [[f64; 6]; 3] {
    let derivative = |y: [[f64; 6]; 3]| {
        let mut dy = [[0.0; 6]; 3];
        for i in 0..3 {
            dy[i][..3].copy_from_slice(&y[i][3..]);
            for j in 0..3 {
                if i == j {
                    continue;
                }
                let distance = (0..3)
                    .map(|axis| (y[j][axis] - y[i][axis]).powi(2))
                    .sum::<f64>();
                let inverse_cube = (distance + initial.softening * initial.softening).powf(-1.5);
                for axis in 0..3 {
                    dy[i][axis + 3] += initial.gravity
                        * initial.bodies[j].mass
                        * (y[j][axis] - y[i][axis])
                        * inverse_cube;
                }
            }
        }
        dy
    };
    let shift = |y: [[f64; 6]; 3], dy: [[f64; 6]; 3], factor: f64| {
        std::array::from_fn(|i| std::array::from_fn(|axis| y[i][axis] + factor * dy[i][axis]))
    };
    let mut y = initial.bodies.map(|b| {
        [
            b.position[0],
            b.position[1],
            b.position[2],
            b.velocity[0],
            b.velocity[1],
            b.velocity[2],
        ]
    });
    let h = duration / steps as f64;
    for _ in 0..steps {
        let a = derivative(y);
        let b = derivative(shift(y, a, h / 2.0));
        let c = derivative(shift(y, b, h / 2.0));
        let d = derivative(shift(y, c, h));
        y = std::array::from_fn(|i| {
            std::array::from_fn(|axis| {
                y[i][axis]
                    + h * (a[i][axis] + 2.0 * b[i][axis] + 2.0 * c[i][axis] + d[i][axis]) / 6.0
            })
        });
    }
    y
}

#[test]
fn spatial_motion_matches_independent_rk4_and_converges() {
    let initial = InitialConditions::default();
    let reference = rk4(&initial, 1.0, 10_000);
    let sim = run(initial.clone(), 1.0, 0.05, 1e-11);
    for (body, expected) in sim.state.bodies.iter().zip(reference) {
        for (value, expected) in body.position.into_iter().chain(body.velocity).zip(expected) {
            assert!((value - expected).abs() < 1e-9, "{value} != {expected}");
        }
    }
    assert!((sim.state.bodies[0].position[2] - initial.bodies[0].position[2]).abs() > 0.01);
    let baseline = Simulation::new(initial.clone(), 0).unwrap().snapshot;
    let regular = run(initial.clone(), 10.0, 0.05, 1e-11);
    let strict = run(initial.clone(), 10.0, 0.05, 1e-13);
    let alternate = run(initial, 10.0, 0.017, 1e-13);
    for result in [&regular, &strict, &alternate] {
        assert!(result.snapshot.relative_energy_error() < 1e-7);
        for axis in 0..3 {
            assert!((result.snapshot.momentum[axis] - baseline.momentum[axis]).abs() < 1e-9);
            assert!(
                (result.snapshot.angular_momentum[axis] - baseline.angular_momentum[axis]).abs()
                    < 1e-8
            );
        }
    }
    let difference = |a: &Simulation, b: &Simulation| {
        a.state
            .bodies
            .iter()
            .zip(b.state.bodies)
            .flat_map(|(a, b)| {
                a.position
                    .into_iter()
                    .chain(a.velocity)
                    .zip(b.position.into_iter().chain(b.velocity))
                    .map(|(a, b)| (a - b).abs())
            })
            .fold(0.0, f64::max)
    };
    assert!(difference(&regular, &strict) < 1e-5);
    assert!(difference(&strict, &alternate) < 1e-7);
    eprintln!(
        "3D t=10: energy drift {:.3e}, strict {:.3e}, tolerance difference {:.3e}, interval difference {:.3e}",
        regular.snapshot.relative_energy_error(),
        strict.snapshot.relative_energy_error(),
        difference(&regular, &strict),
        difference(&strict, &alternate)
    );
}

#[test]
fn inclined_equilateral_orbit_matches_analytic_solution() {
    let inclination = 0.7_f64;
    let omega = (1.0 / 3.0_f64.sqrt()).sqrt();
    let body = |angle: f64| Body {
        position: [
            angle.cos(),
            angle.sin() * inclination.cos(),
            angle.sin() * inclination.sin(),
        ],
        velocity: [
            -omega * angle.sin(),
            omega * angle.cos() * inclination.cos(),
            omega * angle.cos() * inclination.sin(),
        ],
        mass: 1.0,
    };
    let initial = InitialConditions {
        bodies: std::array::from_fn(|i| body(i as f64 * std::f64::consts::TAU / 3.0)),
        gravity: 1.0,
        softening: 0.0,
    };
    let sim = run(initial, 2.0, 0.05, 1e-11);
    for (i, actual) in sim.state.bodies.iter().enumerate() {
        let expected = body(i as f64 * std::f64::consts::TAU / 3.0 + 2.0 * omega);
        for axis in 0..3 {
            assert!((actual.position[axis] - expected.position[axis]).abs() < 1e-9);
            assert!((actual.velocity[axis] - expected.velocity[axis]).abs() < 1e-9);
        }
    }
}

#[test]
fn validation_budget_and_numerical_failure_preserve_valid_state() {
    let mut invalid = InitialConditions::default();
    invalid.bodies[0].position[2] = f64::NAN;
    assert!(Simulation::new(invalid, 0).is_err());
    let mut overlap = InitialConditions::default();
    overlap.bodies[0].position = overlap.bodies[1].position;
    assert!(Simulation::new(overlap.clone(), 0).is_err());
    overlap.softening = 0.1;
    assert!(Simulation::new(overlap, 0).is_ok());
    let mut sim = Simulation::new(InitialConditions::default(), 0).unwrap();
    sim.advance(&FrameParams {
        duration: 1.0,
        max_step: 1e-7,
        ..Default::default()
    });
    assert!(sim.snapshot.time > 0.0 && sim.snapshot.time < 1.0);
    assert_ne!(sim.snapshot.flags & 2, 0);
    assert!(sim.snapshot.steps <= MAX_ATTEMPTS);
    let before = sim.state.clone();
    sim.advance(&FrameParams {
        duration: 0.1,
        rtol: f64::NAN,
        ..Default::default()
    });
    assert!(sim.snapshot.error.is_some());
    assert_eq!(sim.state, before);
    let mut sim = Simulation::new(InitialConditions::default(), 0).unwrap();
    sim.snapshot.time = 1e30;
    sim.advance(&FrameParams {
        duration: 0.1,
        ..Default::default()
    });
    assert!(sim.snapshot.error.is_some());
    assert_eq!(sim.snapshot.time, 1e30);
}
