use super::*;

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual:.17e} != {expected:.17e}"
    );
}

#[test]
fn invalid_inputs_and_unrepresentable_mass_ratios_are_rejected() {
    for mass in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e-320,
        1e308,
    ] {
        for index in 0..2 {
            let mut settings = Settings::default();
            settings.masses[index] = mass;
            assert!(Simulation::new(settings).is_err(), "mass {mass} at {index}");
        }
    }
    assert!(
        Simulation::new(Settings {
            masses: [1e-200, 1e200],
            ..Default::default()
        })
        .is_err()
    );
    for (speed, restitution) in [
        (f64::NAN, 1.0),
        (0.0, 1.0),
        (3.1, 1.0),
        (1.0, f64::NAN),
        (1.0, -0.1),
        (1.0, 1.1),
    ] {
        assert!(
            Simulation::new(Settings {
                incoming_speed: speed,
                restitution,
                ..Default::default()
            })
            .is_err()
        );
    }
}

#[test]
fn masses_and_diagnostics_keep_f64_precision_and_large_dynamic_range() {
    for mass in [0.1, 1.0000000001, 1e-100, 1e100, 1e307] {
        let mut simulation = Simulation::new(Settings {
            masses: [mass; 2],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(simulation.initial_energy, 0.5 * mass);
        simulation.advance(0.5, 10);
        assert!(simulation.error.is_none());
        assert_eq!(simulation.snapshot.kinetic, 0.5 * mass);
        assert_eq!(simulation.settings.masses, [mass; 2]);
    }
}

#[test]
fn first_impact_time_positions_momentum_restitution_and_energy_loss() {
    for masses in [
        [1.0, 1.0],
        [1.0, 100.0],
        [100.0, 1.0],
        [0.25, 2.5],
        [2500.5, 125.25],
        [0.1, 0.3],
    ] {
        for restitution in [0.0, 0.5, 1.0] {
            let settings = Settings {
                masses,
                incoming_speed: 3.0,
                restitution,
            };
            let mut simulation = Simulation::new(settings).unwrap();
            let impact = (START_X[1] - START_X[0] - (SIZES[0] + SIZES[1]) * 0.5) / 3.0;
            simulation.advance(impact - 1e-6, 10);
            assert_eq!(simulation.snapshot.collisions, [0; 2]);
            close(
                simulation.snapshot.x[1],
                START_X[1] - 3.0 * (impact - 1e-6),
                1e-15,
            );
            simulation.advance(2e-6, 10);
            assert!(simulation.error.is_none(), "{:?}", simulation.error);
            assert_eq!(simulation.snapshot.collisions, [1, 0]);
            let [m, big] = masses;
            let expected = [
                -(1.0 + restitution) * big * 3.0 / (m + big),
                -(big - restitution * m) * 3.0 / (m + big),
            ];
            for (actual, expected) in simulation.snapshot.velocity.into_iter().zip(expected) {
                close(actual, expected, 3e-15);
            }
            let [u, v] = simulation.snapshot.velocity;
            close(m * u + big * v, -3.0 * big, 3e-15 * 3.0 * big);
            close(v - u, 3.0 * restitution, 2e-15);
            close(
                simulation.snapshot.kinetic + simulation.snapshot.dissipated,
                simulation.initial_energy,
                simulation.initial_energy * 3e-15,
            );
            close(
                simulation.snapshot.dissipated,
                0.5 * m * big / (m + big) * (1.0 - restitution * restitution) * 9.0,
                simulation.initial_energy * 3e-15,
            );
        }
    }
}

#[test]
fn galperin_collision_counts_and_elastic_energy() {
    // Independent geometric result: ceil(pi / atan(sqrt(m_left/m_right))) - 1.
    // For powers of 100 these are the familiar leading digits of pi.
    for (ratio, expected) in [
        (1.0, 3),
        (100.0, 31),
        (10_000.0, 314),
        (1_000_000.0, 3141),
        (100_000_000.0, 31415),
    ] {
        let mut simulation = Simulation::new(Settings {
            masses: [1.0, ratio],
            ..Default::default()
        })
        .unwrap();
        // The final slow wall rebound can occur long after the dense burst.
        let result = simulation.advance(10_000.0, 40_000);
        eprintln!(
            "ratio {ratio}: count {}, relative energy error {:.3e}, error {:?}",
            simulation.snapshot.total_collisions(),
            simulation.snapshot.kinetic / simulation.initial_energy - 1.0,
            simulation.error
        );
        assert!(simulation.error.is_none());
        assert!(!result.budget_limited);
        assert_eq!(simulation.snapshot.total_collisions(), expected);
        assert_eq!(
            simulation.snapshot.collisions,
            [expected.div_ceil(2), expected / 2]
        );
        assert!(simulation.snapshot.velocity[0] >= 0.0);
        assert!(simulation.snapshot.velocity[1] >= simulation.snapshot.velocity[0]);
        close(
            simulation.snapshot.kinetic / simulation.initial_energy,
            1.0,
            1e-10,
        );
        assert_eq!(simulation.snapshot.dissipated, 0.0);
    }
}

#[test]
fn wall_impact_matches_restitution_and_energy_loss() {
    for restitution in [0.0, 0.5, 1.0] {
        let mut simulation = Simulation::new(Settings {
            restitution,
            ..Default::default()
        })
        .unwrap();
        simulation.anchor.velocity = [-1.0, 0.0];
        simulation.anchor.gaps = [1.0, 10.0];
        simulation.snapshot = simulation.anchor.snapshot(simulation.settings.masses);
        simulation.initial_energy = simulation.snapshot.kinetic;
        simulation.advance(1.0, 5);
        assert!(simulation.error.is_none());
        assert_eq!(simulation.snapshot.collisions, [0, 1]);
        assert_eq!(simulation.snapshot.velocity, [restitution, 0.0]);
        assert_eq!(simulation.snapshot.x[0], SIZES[0] * 0.5);
        close(
            simulation.snapshot.kinetic + simulation.snapshot.dissipated,
            0.5,
            1e-15,
        );
    }
}

#[test]
fn completely_inelastic_pair_sticks_and_stops_at_wall_without_recounts() {
    let mut simulation = Simulation::new(Settings {
        restitution: 0.0,
        ..Default::default()
    })
    .unwrap();
    simulation.advance(2.0, 10);
    assert_eq!(
        simulation.snapshot.velocity[0],
        simulation.snapshot.velocity[1]
    );
    assert_eq!(simulation.snapshot.collisions, [1, 0]);
    simulation.advance(10.0, 10);
    assert!(simulation.error.is_none(), "{:?}", simulation.error);
    assert_eq!(simulation.snapshot.velocity, [0.0; 2]);
    assert_eq!(simulation.snapshot.collisions, [1, 1]);
    close(
        simulation.snapshot.dissipated,
        simulation.initial_energy,
        1e-14,
    );
    let before = simulation.snapshot;
    simulation.advance(10.0, 10);
    assert_eq!(simulation.snapshot.collisions, before.collisions);
    assert_eq!(simulation.snapshot.x, before.x);
}

#[test]
fn dissipative_collapse_stops_explicitly_without_fabricating_rest_or_counts() {
    let mut simulation = Simulation::new(Settings {
        restitution: 0.5,
        ..Default::default()
    })
    .unwrap();
    simulation.advance(12.0, 2048);
    assert!(
        simulation
            .error
            .as_deref()
            .is_some_and(|e| e.contains("f64 resolution")),
        "{:?}",
        simulation.error
    );
    assert!(simulation.snapshot.total_collisions() > 2);
    assert!(simulation.snapshot.total_collisions() < 2048);
    assert!(simulation.snapshot.kinetic < simulation.initial_energy);
    close(
        (simulation.snapshot.kinetic + simulation.snapshot.dissipated) / simulation.initial_energy,
        1.0,
        1e-14,
    );
    let before = simulation.snapshot;
    simulation.advance(100.0, 2048);
    assert_eq!(simulation.snapshot, before);
}

#[test]
fn impact_budget_retains_last_event_and_has_no_deferred_catch_up() {
    let mut simulation = Simulation::new(Settings::default()).unwrap();
    let result = simulation.advance(12.0, 3);
    assert!(result.budget_limited);
    assert_eq!(result.impacts, 3);
    assert_eq!(simulation.snapshot.total_collisions(), 3);
    assert!(simulation.snapshot.time < 12.0);
    let before = simulation.snapshot;
    simulation.advance(0.0, 2048);
    assert_eq!(simulation.snapshot, before);
    simulation.advance(12.0 - before.time, 2048);
    assert_eq!(simulation.snapshot.total_collisions(), 31);
}

#[test]
fn rendering_frame_partitions_do_not_change_impact_sequence_or_velocities() {
    let mut a = Simulation::new(Settings::default()).unwrap();
    let mut b = Simulation::new(Settings::default()).unwrap();
    a.advance(12.0, 2048);
    for _ in 0..1200 {
        b.advance(0.003, 2048);
        b.advance(0.007, 2048);
    }
    assert!(b.error.is_none(), "{:?}", b.error);
    assert_eq!(a.snapshot.collisions, b.snapshot.collisions);
    assert_eq!(a.snapshot.velocity, b.snapshot.velocity);
    assert_eq!(a.snapshot.kinetic, b.snapshot.kinetic);
    for (x, y) in a.snapshot.x.into_iter().zip(b.snapshot.x) {
        close(x, y, 1e-10);
    }
}

#[test]
fn range_failure_preserves_last_valid_snapshot_and_reset_recovers() {
    let mut simulation = Simulation::new(Settings::default()).unwrap();
    let before = simulation.snapshot;
    simulation.anchor.gaps[0] = 1e13;
    simulation.advance(0.01, 2048);
    assert!(simulation.error.is_some());
    assert_eq!(simulation.snapshot, before);
    simulation = Simulation::new(simulation.settings.clone()).unwrap();
    simulation.advance(0.01, 2048);
    assert!(simulation.error.is_none());
}

#[test]
fn simultaneous_elastic_impacts_are_reported_without_an_arbitrary_solve_order() {
    let mut simulation = Simulation::new(Settings::default()).unwrap();
    simulation.anchor.gaps = [1.0, 1.0];
    simulation.anchor.velocity = [-1.0, -2.0];
    let before = simulation.snapshot;
    simulation.advance(2.0, 10);
    assert!(
        simulation
            .error
            .as_deref()
            .is_some_and(|e| e.contains("Simultaneous"))
    );
    assert_eq!(simulation.snapshot, before);
}

#[test]
fn simultaneous_completely_inelastic_impacts_stop_and_count_both_contacts() {
    let mut simulation = Simulation::new(Settings {
        restitution: 0.0,
        ..Default::default()
    })
    .unwrap();
    simulation.anchor.gaps = [1.0, 1.0];
    simulation.anchor.velocity = [-1.0, -2.0];
    simulation.snapshot = simulation.anchor.snapshot(simulation.settings.masses);
    simulation.initial_energy = simulation.snapshot.kinetic;
    let result = simulation.advance(2.0, 1);
    assert!(result.budget_limited, "two simultaneous impacts are atomic");
    assert_eq!(simulation.snapshot.total_collisions(), 0);
    let result = simulation.advance(2.0, 2);
    assert!(!result.budget_limited);
    assert_eq!(result.impacts, 2);
    assert!(simulation.error.is_none());
    assert_eq!(simulation.snapshot.collisions, [1, 1]);
    assert_eq!(simulation.snapshot.velocity, [0.0; 2]);
    assert_eq!(simulation.snapshot.dissipated, simulation.initial_energy);
}

#[test]
fn invalid_durations_do_not_change_the_state_and_time_underflow_stops() {
    let mut simulation = Simulation::new(Settings::default()).unwrap();
    let before = simulation.snapshot;
    for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        simulation.advance(duration, 10);
        assert_eq!(simulation.snapshot, before);
        assert!(simulation.error.is_none());
    }
    simulation.advance(1.0, 10);
    let before = simulation.snapshot;
    simulation.advance(f64::MIN_POSITIVE, 10);
    assert!(simulation.error.is_some());
    assert_eq!(simulation.snapshot, before);
}
