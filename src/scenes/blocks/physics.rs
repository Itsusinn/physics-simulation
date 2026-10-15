//! Avian owns collision detection and impulse resolution. This private Bevy
//! world has no renderer, window, automatic clock, or catch-up schedule.
use std::time::Duration;

use avian2d::dynamics::{joints::joint_graph::JointGraphChange, solver::SolverConfig};
use avian2d::{physics_transform::PhysicsTransformConfig, prelude::*};
use bevy::{ecs::schedule::ScheduleLabel, math::DVec2, prelude::*};

pub const DT: f64 = 0.001;
pub const SIZES: [f64; 2] = [0.7, 1.4];
pub const START_X: [f64; 2] = [2.0, 5.0];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // Avian stores Mass as f32 even in f64 mode. Integer masses in this
    // range are exactly representable; diagnostics use the same applied mass.
    pub masses: [u32; 2],
    pub incoming_speed: f64,
    pub restitution: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            masses: [1, 100],
            incoming_speed: 1.0,
            restitution: 1.0,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.masses.iter().any(|&m| !(1..=100).contains(&m)) {
            return Err("Masses must be whole numbers from 1 to 100.");
        }
        if !self.incoming_speed.is_finite() || !(0.1..=3.0).contains(&self.incoming_speed) {
            return Err("Incoming speed must be from 0.1 to 3.");
        }
        if !self.restitution.is_finite() || !(0.0..=1.0).contains(&self.restitution) {
            return Err("Restitution must be from 0 to 1.");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub x: [f64; 2],
    pub velocity: [f64; 2],
    pub time: f64,
    pub kinetic: f64,
    pub collisions: [u64; 2], // block/block, small block/wall
}

impl Snapshot {
    pub fn total_collisions(&self) -> u64 {
        self.collisions.iter().sum()
    }
}

#[derive(Clone, Copy, Component, PartialEq, Eq)]
enum Part {
    Small,
    Large,
    Wall,
}

#[derive(Resource, Default)]
struct Counts([u64; 2]);

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct AdvanceBlocks;

pub struct Simulation {
    world: World,
    bodies: [Entity; 2],
    pub settings: Settings,
    pub snapshot: Snapshot,
    pub initial_energy: f64,
    pub error: Option<String>,
    ticks: u64,
}

impl Simulation {
    pub fn new(settings: Settings) -> Result<Self, &'static str> {
        settings.validate()?;
        let initial_velocity = -f64::from(settings.incoming_speed as f32);
        let mut app = App::new();
        // Register only the contact pipeline. Building the default plugin group
        // and disabling plugins afterwards still retains their dynamic plugin
        // constructors in WASM (including unused raycasts and joint support).
        app.add_plugins((
            PhysicsSchedulePlugin::new(AdvanceBlocks),
            MassPropertyPlugin::new(AdvanceBlocks),
            ForcePlugin,
            ColliderHierarchyPlugin,
            ColliderTransformPlugin::new(AdvanceBlocks),
            ColliderBackendPlugin::<Collider>::new(AdvanceBlocks),
            ColliderTreePlugin::<Collider>::default(),
            NarrowPhasePlugin::<Collider>::default(),
            BodySizeMetricsPlugin::<Collider>::default(),
        ));
        app.add_plugins((
            SolverBodyPlugin,
            SolverSchedulePlugin,
            IntegratorPlugin::default(),
            SolverPlugin::default(),
            CcdPlugin,
            IslandPlugin,
            IslandSleepingPlugin,
            BroadPhaseCorePlugin,
            BvhBroadPhasePlugin::<()>::default(),
            PhysicsTransformPlugin::new(AdvanceBlocks),
        ));
        app.init_resource::<Time>()
            // SolverPlugin reads this queue even in a world with no joints.
            .add_message::<JointGraphChange>()
            .init_resource::<Time<Real>>()
            .init_resource::<Counts>()
            .insert_resource(Gravity::ZERO)
            // Resolve simultaneous wall/block contacts across a 100:1 mass ratio.
            .insert_resource(SubstepCount(8))
            .insert_resource(SolverConfig {
                restitution_threshold: 0.0,
                // Avoid energetic separation kicks in inelastic contact stacks.
                max_overlap_solve_speed: 0.1,
                restitution_iterations: 8,
                ..Default::default()
            })
            .insert_resource(PhysicsTransformConfig {
                propagate_before_physics: false,
                transform_to_position: false,
                position_to_transform: false,
                transform_to_collider_scale: false,
            })
            .add_systems(
                AdvanceBlocks,
                count_collisions.after(PhysicsSystems::StepSimulation),
            );
        app.finish();
        app.cleanup();
        let mut world = std::mem::take(app.world_mut());
        let wall = (
            RigidBody::Static,
            Friction::ZERO,
            Restitution::new(settings.restitution as f32),
        );
        world.spawn((
            wall,
            Part::Wall,
            Position(DVec2::new(-0.15, 1.7)),
            Collider::rectangle(0.3, 3.4),
        ));
        // Horizontal leg of the L. The slider constraint locks vertical motion;
        // gravity is zero, so the rail does not introduce resting-contact noise.
        world.spawn((
            RigidBody::Static,
            Position(DVec2::new(49.85, -0.15)),
            Collider::rectangle(100.3, 0.3),
            Friction::ZERO,
        ));
        let bodies = std::array::from_fn(|i| {
            world
                .spawn((
                    if i == 0 { Part::Small } else { Part::Large },
                    RigidBody::Dynamic,
                    Collider::rectangle(SIZES[i] as f32, SIZES[i] as f32),
                    Position(DVec2::new(START_X[i], SIZES[i] * 0.5)),
                    LinearVelocity(Vec2::new(
                        if i == 0 {
                            0.0
                        } else {
                            -settings.incoming_speed as f32
                        },
                        0.0,
                    )),
                    Mass(settings.masses[i] as f32),
                    NoAutoMass,
                    LockedAxes::ROTATION_LOCKED.lock_translation_y(),
                    Friction::ZERO,
                    Restitution::new(settings.restitution as f32),
                    SleepingDisabled,
                    CollisionEventsEnabled,
                    SpeculativeCcd::ZERO,
                    SweptCcd::default()
                        .with_filter(CcdFilter::ALL)
                        .with_threshold(0.0),
                ))
                .id()
        });
        // Initialize collider/mass components without advancing simulation time.
        world.run_schedule(AdvanceBlocks);
        let kinetic = 0.5 * f64::from(settings.masses[1]) * initial_velocity.powi(2);
        Ok(Self {
            world,
            bodies,
            snapshot: Snapshot {
                x: START_X,
                velocity: [0.0, initial_velocity],
                time: 0.0,
                kinetic,
                collisions: [0; 2],
            },
            settings,
            initial_energy: kinetic,
            error: None,
            ticks: 0,
        })
    }

    pub fn tick(&mut self) {
        if self.error.is_some() {
            return;
        }
        self.world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1));
        self.world.run_schedule(AdvanceBlocks);
        let x = self
            .bodies
            .map(|e| self.world.get::<Position>(e).expect("slider position").x);
        let velocity = self.bodies.map(|e| {
            f64::from(
                self.world
                    .get::<LinearVelocity>(e)
                    .expect("slider velocity")
                    .x,
            )
        });
        let kinetic = (0..2)
            .map(|i| 0.5 * f64::from(self.settings.masses[i]) * velocity[i].powi(2))
            .sum::<f64>();
        if x.iter()
            .chain(velocity.iter())
            .chain([&kinetic])
            .any(|v| !v.is_finite() || v.abs() > 1e12)
            || x[0] < SIZES[0] * 0.5 - 0.02
            || x[1] - x[0] < (SIZES[0] + SIZES[1]) * 0.5 - 0.02
        {
            self.error = Some("Collision solver left the valid range. Last valid frame retained; Reset to recover.".into());
            return;
        }
        self.ticks += 1;
        self.snapshot = Snapshot {
            x,
            velocity,
            kinetic,
            time: self.ticks as f64 * DT,
            collisions: self.world.resource::<Counts>().0,
        };
        // There is no App::update driving message maintenance in this private
        // world. Explicit rotation keeps collision messages bounded while idle.
        self.world
            .resource_mut::<Messages<CollisionStart>>()
            .update();
        self.world.resource_mut::<Messages<CollisionEnd>>().update();
    }
}

fn count_collisions(
    mut events: MessageReader<CollisionStart>,
    parts: Query<&Part>,
    mut counts: ResMut<Counts>,
) {
    for event in events.read() {
        let (Ok(a), Ok(b)) = (parts.get(event.collider1), parts.get(event.collider2)) else {
            continue;
        };
        match (*a, *b) {
            (Part::Small, Part::Large) | (Part::Large, Part::Small) => counts.0[0] += 1,
            (Part::Small, Part::Wall) | (Part::Wall, Part::Small) => counts.0[1] += 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_impact_matches_one_dimensional_impulse_equations() {
        for masses in [[1, 1], [1, 100], [100, 1]] {
            for restitution in [0.0, 0.5, 1.0] {
                let settings = Settings {
                    masses,
                    incoming_speed: 3.0,
                    restitution,
                };
                let mut simulation = Simulation::new(settings).unwrap();
                for _ in 0..800 {
                    simulation.tick();
                    if simulation.snapshot.collisions[0] > 0 {
                        break;
                    }
                }
                // Contact events may precede the impulse by one step because
                // the narrow phase includes a small contact tolerance. Allow
                // the contact solve to settle, well before the wall is reached.
                for _ in 0..50 {
                    simulation.tick();
                }
                assert!(simulation.error.is_none(), "{:?}", simulation.error);
                assert_eq!(simulation.snapshot.collisions, [1, 0]);
                let [m, big] = masses.map(f64::from);
                let expected = [
                    -(1.0 + restitution) * big * 3.0 / (m + big),
                    -(big - restitution * m) * 3.0 / (m + big),
                ];
                for (actual, expected) in simulation.snapshot.velocity.into_iter().zip(expected) {
                    assert!(
                        (actual - expected).abs() < 1e-5,
                        "masses {masses:?}, e {restitution}: {actual} vs {expected}"
                    );
                }
                let momentum =
                    m * simulation.snapshot.velocity[0] + big * simulation.snapshot.velocity[1];
                assert!((momentum + big * 3.0).abs() < 1e-3);
                assert!(simulation.snapshot.kinetic <= simulation.initial_energy * (1.0 + 1e-5));
            }
        }
    }

    #[test]
    fn avian_resolves_block_and_wall_collisions() {
        let mut simulation = Simulation::new(Settings::default()).unwrap();
        for _ in 0..12_000 {
            simulation.tick();
        }
        eprintln!("{:?}, error {:?}", simulation.snapshot, simulation.error);
        assert!(simulation.error.is_none());
        assert_eq!(simulation.snapshot.collisions, [16, 15]);
        assert_eq!(simulation.snapshot.total_collisions(), 31);
        assert!(simulation.snapshot.velocity[1] > 0.0);
        assert!((simulation.snapshot.kinetic / simulation.initial_energy - 1.0).abs() < 1e-4);
    }

    #[test]
    fn wall_reverses_velocity_and_failure_retains_last_snapshot() {
        let mut simulation = Simulation::new(Settings::default()).unwrap();
        simulation
            .world
            .entity_mut(simulation.bodies[0])
            .insert(LinearVelocity(Vec2::new(-1.0, 0.0)));
        simulation
            .world
            .entity_mut(simulation.bodies[1])
            .insert(LinearVelocity::ZERO);
        for _ in 0..1800 {
            simulation.tick();
        }
        assert!(simulation.error.is_none());
        assert_eq!(simulation.snapshot.collisions, [0, 1]);
        assert!((simulation.snapshot.velocity[0] - 1.0).abs() < 1e-5);
        let before = simulation.snapshot;
        simulation
            .world
            .entity_mut(simulation.bodies[0])
            .insert(Position(DVec2::new(-1.0, 0.35)));
        simulation.tick();
        assert!(simulation.error.is_some());
        assert_eq!(simulation.snapshot, before);
        simulation.tick();
        assert_eq!(simulation.snapshot, before);
    }

    #[test]
    fn maximum_speed_and_dissipative_contacts_remain_bounded() {
        for restitution in [0.0, 0.5, 1.0] {
            let mut simulation = Simulation::new(Settings {
                incoming_speed: 3.0,
                restitution,
                ..Default::default()
            })
            .unwrap();
            for _ in 0..6_000 {
                simulation.tick();
            }
            assert!(
                simulation.error.is_none(),
                "e {restitution}: {:?}",
                simulation.error
            );
            assert!(simulation.snapshot.collisions.iter().all(|&n| n > 0));
            assert!(
                simulation.snapshot.kinetic <= simulation.initial_energy * 1.0001,
                "e {restitution}: {:?}",
                simulation.snapshot
            );
            if restitution == 1.0 {
                assert_eq!(simulation.snapshot.collisions, [16, 15]);
                assert!(simulation.snapshot.velocity[1] > 0.0);
            } else {
                assert!(simulation.snapshot.kinetic < simulation.initial_energy * 0.9);
            }
        }
    }
}
