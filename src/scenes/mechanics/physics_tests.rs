use super::*;

fn integrate(sim: &mut Simulation, end: f64, frame: f64, params: &FrameParams) {
    while sim.time < end {
        sim.advance((end - sim.time).min(frame), params);
        assert!(sim.error.is_none(), "{:?}", sim.error);
        assert!(!sim.limited);
    }
}

#[test]
fn all_presets_conserve_energy_over_twenty_time_units() {
    for kind in [
        Kind::DoublePendulum,
        Kind::CoupledSprings,
        Kind::KeplerOrbit,
    ] {
        for index in 0..kind.presets().len() {
            let mut sim = Simulation::new(Model::preset(kind, index)).unwrap();
            let mut max_drift = 0.0_f64;
            for _ in 0..400 {
                sim.advance(0.05, &FrameParams::default());
                assert!(sim.error.is_none(), "{:?}", sim.error);
                max_drift = max_drift.max(
                    ((sim.model.energy(&sim.state) - sim.initial_energy) / sim.energy_scale).abs(),
                );
            }
            println!("{kind:?} preset {index}: maximum scaled energy drift {max_drift:.3e}");
            assert!(max_drift < 2e-8, "{kind:?}: {max_drift}");
        }
    }
}

#[test]
fn spring_normal_modes_match_analytic_positions_and_velocities() {
    for preset in [1, 2] {
        let mut sim = Simulation::new(Model::preset(Kind::CoupledSprings, preset)).unwrap();
        let omega = if preset == 1 { 1.0_f64 } else { 1.5_f64.sqrt() };
        let sign = if preset == 1 { 1.0 } else { -1.0 };
        integrate(&mut sim, 20.0, 0.037, &FrameParams::default());
        let q = (omega * sim.time).cos();
        let v = -omega * (omega * sim.time).sin();
        assert!((sim.state - State::from([q, sign * q, v, sign * v])).amax() < 1e-10);
    }
}

#[test]
fn uncoupled_springs_keep_unexcited_mass_stationary() {
    let mut model = Model::preset(Kind::CoupledSprings, 0);
    model.parameters[3] = 0.0;
    model.parameters[0] = 2.0;
    let mut sim = Simulation::new(model).unwrap();
    integrate(&mut sim, 10.0, 0.05, &FrameParams::default());
    assert_eq!(sim.state[1], 0.0);
    assert_eq!(sim.state[3], 0.0);
    assert!((sim.state[0] - (10.0 / 2.0_f64.sqrt()).cos()).abs() < 1e-10);
}

#[test]
fn kepler_circular_and_elliptical_orbits_close_and_preserve_angular_momentum() {
    for preset in [0, 1] {
        let mut sim = Simulation::new(Model::preset(Kind::KeplerOrbit, preset)).unwrap();
        let semi_major = -sim.model.parameters[0] / (2.0 * sim.initial_energy);
        let period = std::f64::consts::TAU * semi_major.powf(1.5) / sim.model.parameters[0].sqrt();
        let h0 = sim.state[0] * sim.state[3] - sim.state[1] * sim.state[2];
        integrate(&mut sim, period, 0.043, &FrameParams::default());
        assert!((sim.state - sim.model.initial).amax() < 1e-8);
        let h = sim.state[0] * sim.state[3] - sim.state[1] * sim.state[2];
        assert!((h - h0).abs() < 1e-10);
    }
}

// Independent explicit double-pendulum equations, unlike the production mass matrix.
fn pendulum_rhs(model: &Model, y: State) -> State {
    let [m1, m2, l1, l2, g] = model.parameters;
    let delta = y[0] - y[1];
    let denominator = 2.0 * m1 + m2 - m2 * (2.0 * delta).cos();
    let a1 = (-g * (2.0 * m1 + m2) * y[0].sin()
        - m2 * g * (y[0] - 2.0 * y[1]).sin()
        - 2.0 * delta.sin() * m2 * (y[3].powi(2) * l2 + y[2].powi(2) * l1 * delta.cos()))
        / (l1 * denominator);
    let a2 = 2.0
        * delta.sin()
        * (y[2].powi(2) * l1 * (m1 + m2)
            + g * (m1 + m2) * y[0].cos()
            + y[3].powi(2) * l2 * m2 * delta.cos())
        / (l2 * denominator);
    State::from([y[2], y[3], a1, a2])
}

#[test]
fn pendulum_matches_independent_rk4_and_converges_with_tighter_tolerances() {
    let mut model = Model::preset(Kind::DoublePendulum, 0);
    model.parameters = [1.7, 0.6, 1.3, 0.8, 9.81];
    model.initial[2] = 0.3;
    let mut reference = model.initial;
    let h = 0.0001;
    for _ in 0..10000 {
        let k1 = pendulum_rhs(&model, reference);
        let k2 = pendulum_rhs(&model, reference + k1 * (h / 2.0));
        let k3 = pendulum_rhs(&model, reference + k2 * (h / 2.0));
        let k4 = pendulum_rhs(&model, reference + k3 * h);
        reference += (k1 + k2 * 2.0 + k3 * 2.0 + k4) * (h / 6.0);
    }
    let mut normal = Simulation::new(model.clone()).unwrap();
    let mut strict = Simulation::new(model.clone()).unwrap();
    integrate(&mut normal, 1.0, 0.05, &FrameParams::default());
    let params = FrameParams {
        rtol: 1e-13,
        atol: 1e-15,
        ..Default::default()
    };
    integrate(&mut strict, 1.0, 0.017, &params);
    assert!((normal.state - reference).amax() < 1e-8);
    assert!((strict.state - reference).amax() < 1e-10);
    let mut equilibrium = model;
    equilibrium.initial.fill(0.0);
    let mut sim = Simulation::new(equilibrium).unwrap();
    integrate(&mut sim, 1.0, 0.05, &params);
    assert_eq!(sim.state, State::zeros());
}

#[test]
fn rejects_invalid_parameters_and_stops_at_singular_boundary() {
    for kind in [
        Kind::DoublePendulum,
        Kind::CoupledSprings,
        Kind::KeplerOrbit,
    ] {
        let mut model = Model::preset(kind, 0);
        model.parameters[0] = 0.0;
        assert!(Simulation::new(model).is_err());
        let mut model = Model::preset(kind, 0);
        model.initial[0] = f64::NAN;
        assert!(Simulation::new(model).is_err());
    }
    let mut model = Model::preset(Kind::KeplerOrbit, 0);
    model.initial.fill(0.0);
    assert!(Simulation::new(model.clone()).is_err());
    model.initial[0] = 1e-4;
    let mut sim = Simulation::new(model).unwrap();
    for _ in 0..10 {
        sim.advance(0.05, &FrameParams::default());
        if sim.error.is_some() {
            break;
        }
    }
    assert!(sim.error.is_some());
    assert!(sim.model.valid_state(&sim.state));
    let last = sim.state;
    sim.advance(0.05, &FrameParams::default());
    assert_eq!(last, sim.state);
}

#[test]
fn work_limit_keeps_accepted_progress_and_invalid_time_preserves_state() {
    let mut sim = Simulation::new(Model::preset(Kind::CoupledSprings, 0)).unwrap();
    let params = FrameParams {
        max_step: 0.001,
        ..Default::default()
    };
    sim.advance_with_budget(1.0, &params, 2);
    assert!(sim.limited);
    assert!(sim.time > 0.0 && sim.time < 1.0);
    assert!(sim.error.is_none());
    let last = sim.state;
    sim.advance(f64::NAN, &params);
    assert!(sim.error.is_some());
    assert_eq!(last, sim.state);
    let mut sim = Simulation::new(Model::preset(Kind::CoupledSprings, 0)).unwrap();
    sim.time = 1e30;
    sim.advance(0.05, &params);
    assert!(sim.error.is_some());
    assert_eq!(sim.state, sim.model.initial);
}
