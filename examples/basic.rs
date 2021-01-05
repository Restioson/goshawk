use bevy::prelude::*;
use bevy::render::mesh::shape::Cube;
use bevy::pbr::PbrBundle;
use bevy::window::WindowMode;
use bevy_rts_camera::{rts_camera_system, RtsCamera, ZoomSettings, PanSettings};
use itertools::Itertools;

fn main() {
    App::build()
        .add_resource(WindowDescriptor {
            vsync: false,
            resizable: false,
            title: "Goshawk Example".to_string(),
            mode: WindowMode::BorderlessFullscreen,
            ..Default::default()
        })
        .add_resource(Msaa { samples: 8 })
        .add_plugins(DefaultPlugins)
        .add_system(rts_camera_system.system())
        .add_system(exit_on_esc.system())
        .add_startup_system(setup.system())
        .run()
}

fn exit_on_esc(input: Res<Input<KeyCode>>, _query: Query<()>) {
    if input.pressed(KeyCode::Escape) {
        std::process::exit(0);
    }
}

fn setup(commands: &mut Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mesh = meshes.add(Mesh::from(Cube::new(5.0)));
    let material = materials.add(StandardMaterial {
        albedo: Color::BEIGE,
        ..Default::default()
    });

    let intervals = (0..=100).step_by(10);

    commands.spawn_batch(
        intervals
            .clone()
            .cartesian_product(intervals)
            .map(move |(x, z)| PbrBundle {
                mesh: mesh.clone(),
                material: material.clone(),
                transform: Transform::from_translation(Vec3::new(x as f32, 0.0, z as f32)),
                ..Default::default()
            })
    );

    commands
        .spawn(LightBundle {
            light: Light {
                color: Color::hex("efebd8").unwrap(),
                ..Default::default()
            },
            transform: Transform::from_translation(Vec3::new(10.0, 5.0, 10.0)),
            ..Default::default()
        })
        .spawn(Camera3dBundle::default())
        .with(RtsCamera {
            looking_at: Vec3::new(50.0, 0.0, 50.0),
            zoom_distance: 100.0,

            ..Default::default()
        })
        .with(ZoomSettings {
            scroll_accel: 10.0,
            max_velocity: 50.0,
            idle_deceleration: 200.0,
            angle_change_zone: 30.0..=75.0,
            distance_range: 25.0..=100.0,
            ..Default::default()
        })
        .with(PanSettings {
            mouse_accel: 75.0,
            keyboard_accel: 50.0,
            idle_deceleration: 75.0,
            max_speed: 25.0,
            ..Default::default()
        });
}