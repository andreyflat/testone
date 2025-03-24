use bevy::prelude::*;
use crate::player::{Player, WishDirection};

pub struct WorldPlugin;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_floor);
    }
}

#[derive(Component)]
pub struct Ground;

fn spawn_floor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Создаем материалы для шахматной доски
    let dark_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.2, 0.2),
        perceptual_roughness: 0.9,
        metallic: 0.0,
        ..default()
    });
    
    let light_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.8, 0.8),
        perceptual_roughness: 0.9,
        metallic: 0.0,
        ..default()
    });

    // Создаем шахматный паттерн из плоскостей
    let tile_size = 2.0;
    let board_size = 15; // Количество клеток в одну сторону
    let plane_mesh = meshes.add(Plane3d::default().mesh().size(tile_size, tile_size));
    
    for x in -board_size..board_size {
        for z in -board_size..board_size {
            let position = Vec3::new(
                x as f32 * tile_size,
                0.0,
                z as f32 * tile_size
            );
            
            // Выбираем материал в зависимости от четности суммы координат
            let material = if (x + z) % 2 == 0 {
                dark_material.clone()
            } else {
                light_material.clone()
            };
            
            commands.spawn((
                Mesh3d(plane_mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(position),
                Ground,
            ));
        }
    }
}

pub fn draw_cursor(
    camera_query: Query<(&Camera, &GlobalTransform)>,
    ground: Query<&GlobalTransform, With<Ground>>,
    windows: Query<&Window>,
    mut gizmos: Gizmos,
    mut player_query: Query<(&mut Transform, &WishDirection), With<Player>>,
) {
    let (camera, camera_transform) = camera_query.single();
    let ground_transform = if let Some(transform) = ground.iter().next() {
        transform
    } else {
        return;
    };

    let Some(cursor_position) = windows.single().cursor_position() else {
        return;
    };

    // Вычисляем луч из камеры в мир на основе позиции курсора
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Вычисляем точку пересечения луча с плоскостью земли
    let Some(distance) = ray.intersect_plane(
        ground_transform.translation(),
        InfinitePlane3d::new(ground_transform.up())
    ) else {
        return;
    };
    let point = ray.get_point(distance);

    // Рисуем круг чуть выше плоскости земли в этой позиции
    gizmos.circle(
        Isometry3d::new(
            point + ground_transform.up() * 0.01,
            Quat::from_rotation_arc(Vec3::Z, ground_transform.up().as_vec3()),
        ),
        0.2,
        Color::WHITE,
    );

    // Поворачиваем игрока в направлении точки пересечения
    if let Ok((mut player_transform, wish_dir)) = player_query.get_single_mut() {
        let target_point = point;
        let current_pos = player_transform.translation;
        
        // Получаем направление только в горизонтальной плоскости
        let direction = Vec3::new(
            target_point.x - current_pos.x,
            0.0, // Игнорируем вертикальную составляющую
            target_point.z - current_pos.z,
        ).normalize();

        // Стандартное направление "вперед" в Bevy
        let forward = Vec3::NEG_Z;

        if direction != Vec3::ZERO {
            // Создаем поворот от вектора вперед к направлению цели
            let rotation = Quat::from_rotation_arc(forward, direction);
            player_transform.rotation = rotation;
        } else if wish_dir.0 != Vec3::ZERO {
            // Если нет направления к курсору, но игрок движется, поворачиваем его в направлении движения
            let rotation = Quat::from_rotation_arc(forward, wish_dir.0);
            player_transform.rotation = rotation;
        }
    }
}
