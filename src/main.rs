use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera3dBundle::default());
}
fn spawn_scene(mut commands: Commands) {
    // Cube
    commands.spawn(PbrBundle {
        mesh: Mesh::from(shape::Cube { size: 1.0 }),
        material: StandardMaterial {
            base_color: Color::rgb(0.8, 0.2, 0.2),
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.5, 0.0),
        ..default()
    });

    // Light
    commands.spawn(PointLightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
}
