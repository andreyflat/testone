// src/camera.rs - Восстановленная оригинальная камера
use bevy::prelude::*;
use bevy::camera::Hdr;
use bevy::input::mouse::MouseMotion;
use crate::player::{Player, PlayerCamera};

pub struct CameraPlugin;
impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
           .add_systems(Update, (mouse_look, follow_camera));
    }
}

#[derive(Component)]
pub struct CameraSettings {
    pub pitch: f32,
    pub yaw: f32,
    pub sensitivity: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.0,
            sensitivity: 0.1,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Hdr,
        Transform::from_xyz(0.0, 1.7, 0.0),
        CameraSettings::default(),
        PlayerCamera,
    ));
}

pub fn mouse_look(
    mut mouse_motion_events: MessageReader<MouseMotion>,
    mut query: Query<(&mut Transform, &mut CameraSettings), With<PlayerCamera>>,
) {
    let mut delta = Vec2::ZERO;
    for event in mouse_motion_events.read() {
        delta += event.delta;
    }

    if delta == Vec2::ZERO {
        return;
    }

    if let Ok((mut transform, mut settings)) = query.single_mut() {
        settings.yaw -= delta.x * settings.sensitivity;
        settings.pitch -= delta.y * settings.sensitivity;
        settings.pitch = settings.pitch.clamp(-89.0, 89.0);

        transform.rotation = Quat::from_euler(
            EulerRot::YXZ,
            settings.yaw.to_radians(),
            settings.pitch.to_radians(),
            0.0,
        );
    }
}

pub fn follow_camera(
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    
    if let Ok(mut camera_transform) = camera_query.single_mut() {
        camera_transform.translation = player_transform.translation + Vec3::new(0.0, 0.7, 0.0);
    }
}