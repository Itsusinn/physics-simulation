use super::*;
use std::time::Duration;

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Time<Real>>()
        .init_resource::<ThreeBodyApp>()
        .init_resource::<Laboratory>()
        .add_systems(
            Update,
            |time: Res<Time<Real>>, mut lab: ResMut<Laboratory>, mut ui: ResMut<ThreeBodyApp>| {
                advance_simulation(time.delta_secs_f64(), &mut lab, &mut ui);
            },
        );
    app
}

fn tick(app: &mut App, seconds: f64) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs_f64(seconds));
    app.update();
}

#[test]
fn bevy_schedule_preserves_pause_single_step_reset_and_wall_time_cap() {
    let mut app = app();
    tick(&mut app, 1.0);
    let initial = app.world().resource::<Laboratory>().frame.clone();
    assert_eq!(app.world().resource::<Laboratory>().count, 1);
    assert_eq!(
        app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .time,
        0.0
    );
    tick(&mut app, 1.0);
    assert!(Arc::ptr_eq(
        &initial,
        &app.world().resource::<Laboratory>().frame
    ));

    app.world_mut().resource_mut::<ThreeBodyApp>().step = true;
    tick(&mut app, 1.0);
    let lab = app.world().resource::<Laboratory>();
    assert!((lab.simulation.snapshot.time - 0.05).abs() < 1e-15);
    assert_eq!(lab.count, 2);
    assert!(!app.world().resource::<ThreeBodyApp>().step);
    tick(&mut app, 1.0);
    assert_eq!(app.world().resource::<Laboratory>().count, 2);

    app.world_mut().resource_mut::<ThreeBodyApp>().running = true;
    tick(&mut app, 60.0);
    assert!(
        (app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .time
            - 0.1)
            .abs()
            < 1e-14
    );
    tick(&mut app, 0.01);
    assert!(
        (app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .time
            - 0.11)
            .abs()
            < 1e-14
    );

    {
        let mut ui = app.world_mut().resource_mut::<ThreeBodyApp>();
        ui.running = false;
        ui.generation += 1;
        ui.active.bodies[0].velocity[0] = 0.1;
    }
    tick(&mut app, 0.01);
    let lab = app.world().resource::<Laboratory>();
    assert_eq!(lab.simulation.snapshot.time, 0.0);
    assert_eq!(lab.simulation.snapshot.generation, 1);
    assert_eq!(lab.simulation.state.bodies[0].velocity[0], 0.1);
    assert_eq!(lab.count, 1);
    assert_eq!(lab.next, 1);
}

#[test]
fn history_wraps_and_published_frames_remain_immutable() {
    let mut lab = Laboratory::default();
    let first = lab.frame.clone();
    for n in 1..=HISTORY + 3 {
        lab.simulation.state.bodies[0].position[0] = f64::from(n);
        lab.sample();
    }
    lab.publish();
    assert_eq!(lab.count, HISTORY);
    assert_eq!(lab.next, 4);
    assert_eq!(
        f32::from_le_bytes(lab.frame.history[48..52].try_into().unwrap()),
        (HISTORY + 3) as f32
    );
    assert_eq!(
        f32::from_le_bytes(first.history[..4].try_into().unwrap()),
        1.0
    );
    assert_eq!(first.revision, 1);
    assert_eq!(lab.frame.revision, 2);
}

#[test]
fn integration_failure_stops_run_and_reset_recovers() {
    let mut app = app();
    {
        let mut ui = app.world_mut().resource_mut::<ThreeBodyApp>();
        ui.running = true;
        ui.params.rtol = f64::NAN;
    }
    tick(&mut app, 0.01);
    assert!(!app.world().resource::<ThreeBodyApp>().running);
    assert!(
        app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .error
            .is_some()
    );
    {
        let mut ui = app.world_mut().resource_mut::<ThreeBodyApp>();
        ui.params.rtol = 1e-11;
        ui.generation += 1;
        ui.step = true;
    }
    tick(&mut app, 0.01);
    assert!(
        app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .error
            .is_none()
    );
    assert!(
        app.world()
            .resource::<Laboratory>()
            .simulation
            .snapshot
            .time
            > 0.0
    );
}
