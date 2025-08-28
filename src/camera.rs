// src/camera.rs - Восстановленная оригинальная камера
use bevy::prelude::*;
use crate::player::{Player, PlayerCamera};

pub struct CameraPlugin;
impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
           .add_systems(Update, follow_camera);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            hdr: true,
            ..default()
        },
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        FollowCamera {
            distance: 10.0,
            height: 5.0,
            lerp_speed: 5.0,
        },
        PlayerCamera,
    ));
}

#[derive(Component)]
pub struct FollowCamera {
    distance: f32,       // Расстояние от камеры до игрока (по оси Z)
    height: f32,         // Высота камеры над игроком
    lerp_speed: f32,     // Скорость интерполяции
}

pub fn follow_camera(
    time: Res<Time>,
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<(&mut Transform, &FollowCamera), (With<Camera3d>, Without<Player>)>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };
    
    for (mut camera_transform, follow) in camera_query.iter_mut() {
        // Используем только горизонтальные координаты игрока,
        // игнорируем вертикальную составляющую (прыжки)
        let player_position_horizontal = Vec3::new(
            player_transform.translation.x,
            0.0, // Фиксированная базовая высота для игнорирования прыжков
            player_transform.translation.z
        );
        
        // Камера будет находиться позади и на фиксированной высоте
        let new_camera_position = Vec3::new(
            player_position_horizontal.x,
            follow.height, // Фиксированная высота камеры
            player_position_horizontal.z + follow.distance
        );
        
        // Плавно перемещаем камеру к целевой позиции
        camera_transform.translation = camera_transform.translation.lerp(
            new_camera_position, 
            follow.lerp_speed * time.delta_secs()
        );
        
        // Устанавливаем фиксированную ориентацию камеры (смотрит вниз под углом)
        camera_transform.rotation = Quat::from_rotation_x(-0.5);
    }
}