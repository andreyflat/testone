//! A simple 3D scene with light shining over a cube sitting on a plane.

use bevy::{
    prelude::*,
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    window::{WindowMode, PresentMode},
};

mod lights;

// Константы для движения
const MAX_SPEED: f32 = 8.0;
const ACCELERATE: f32 = 16.0;
const AIR_ACCELERATE: f32 = 16.0;
const GRAVITY: f32 = -9.81;
const JUMP_FORCE: f32 = 4.0;
const MAX_JUMP_HEIGHT: f32 = 2.0; // Максимальная высота прыжка
const MAX_AIR_SPEED: f32 = 16.0; // Максимальная скорость в воздухе

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::Windowed,
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (
            player_movement,
            apply_acceleration,
            update_position,
            draw_cursor.after(update_position),
            follow_camera.after(update_position),
            check_exit,
        ).chain())
        .run();
}

#[derive(Component)]
struct Ground;

/// set up a simple 3D scene
fn setup(
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
    
    // cube
    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(0.25, 0.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb_u8(124, 144, 255),
            perceptual_roughness: 0.2,
            metallic: 0.7,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Player,
        Jump::default(),
        Velocity::default(),
        WishDirection::default(),
        WishSpeed::default(),
    ));
    
    // Вызываем функцию из модуля lights для создания света
    lights::spawn_directional_light(&mut commands);
    
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        FollowCamera {
            distance: 10.0,     // Расстояние от куба до камеры
            height: 5.0,        // Высота камеры над кубом
            lerp_speed: 5.0,    // Скорость интерполяции
        },
    ));
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Jump {
    is_jumping: bool,
    can_jump: bool,
    jump_cooldown: f32,
    jump_timer: f32,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            is_jumping: false,
            can_jump: true,
            jump_cooldown: 0.01, // Задержка между прыжками (в секундах)
            jump_timer: 0.0,
        }
    }
}

#[derive(Component, Default)]
struct Velocity(Vec3);

#[derive(Component, Default)]
struct WishDirection(Vec3);

#[derive(Component, Default)]
struct WishSpeed(f32);

// Считываем ввод и обновляем желаемое направление движения
fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    ground: Query<&GlobalTransform, With<Ground>>,
    windows: Query<&Window>,
    mut query: Query<(&Transform, &mut WishDirection, &mut WishSpeed, &mut Jump, &mut Velocity), With<Player>>,
) {
    let (player_transform, mut wishdir, mut wishspeed, mut jump, mut velocity) = query.single_mut();
    let (camera, camera_transform) = camera_query.single();
    let ground_transform = ground.iter().next().unwrap();

    let mut direction = Vec3::ZERO;
    let mut forward_direction = Vec3::ZERO;

    // Получаем позицию курсора и вычисляем точку пересечения с землей
    if let Some(cursor_position) = windows.single().cursor_position() {
        if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) {
            if let Some(distance) = ray.intersect_plane(
                ground_transform.translation(),
                InfinitePlane3d::new(ground_transform.up())
            ) {
                let cursor_world_position = ray.get_point(distance);
                
                // Вычисляем направление к курсору (будет использовано как "вперед")
                let target_position = Vec3::new(
                    cursor_world_position.x,
                    player_transform.translation.y,
                    cursor_world_position.z
                );
                
                forward_direction = (target_position - player_transform.translation).normalize();
                
                // Если нажата клавиша W, двигаемся в направлении курсора
                if keyboard_input.pressed(KeyCode::KeyW) {
                    direction += forward_direction;
                }
            }
        }
    }

    // Если у нас есть направление вперед, считаем направления право и назад
    if forward_direction != Vec3::ZERO {
        // Получаем вектор "вправо" относительно направления к курсору
        let right_direction = Vec3::new(
            -forward_direction.z,
            0.0,
            forward_direction.x
        ).normalize();

        // Движение назад (S) - противоположно направлению вперед
        if keyboard_input.pressed(KeyCode::KeyS) {
            direction -= forward_direction;
        }
        
        // Движение влево (A) - противоположно направлению вправо
        if keyboard_input.pressed(KeyCode::KeyA) {
            direction -= right_direction;
        }
        
        // Движение вправо (D)
        if keyboard_input.pressed(KeyCode::KeyD) {
            direction += right_direction;
        }
    } else {
        // Если направление к курсору не определено, используем мировые координаты
        if keyboard_input.pressed(KeyCode::KeyS) {
            direction.z += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            direction.x -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            direction.x += 1.0;
        }
    }

    if direction.length() > 0.0 {
        direction = direction.normalize();
    }

    wishdir.0 = direction;
    // Устанавливаем желаемую скорость в зависимости от состояния прыжка
    wishspeed.0 = if jump.is_jumping { MAX_AIR_SPEED } else { MAX_SPEED };
    
    // Обработка прыжка
    if keyboard_input.pressed(KeyCode::Space) && !jump.is_jumping && jump.can_jump {
        jump.is_jumping = true;
        jump.can_jump = false;
        jump.jump_timer = jump.jump_cooldown;
        velocity.0.y = JUMP_FORCE;
    }
}

// Применяем ускорение
fn apply_acceleration(
    time: Res<Time>,
    mut query: Query<(&WishDirection, &WishSpeed, &Jump, &mut Velocity, &Transform)>,
) {
    let (wishdir, wishspeed, jump, mut velocity, transform) = query.single_mut();
    
    // Применяем горизонтальное ускорение
    let horizontal_velocity = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
    let currentspeed = horizontal_velocity.dot(wishdir.0);
    let addspeed = wishspeed.0 - currentspeed;

    if addspeed > 0.0 {
        let accel = if !jump.is_jumping { ACCELERATE } else { AIR_ACCELERATE };
        let accelspeed = (accel * time.delta_secs() * wishspeed.0).min(addspeed);

        velocity.0.x += wishdir.0.x * accelspeed;
        velocity.0.z += wishdir.0.z * accelspeed;
        
        // Ограничиваем горизонтальную скорость в зависимости от состояния
        let max_horizontal_speed = if jump.is_jumping { MAX_AIR_SPEED } else { MAX_SPEED };
        let horizontal_speed = Vec3::new(velocity.0.x, 0.0, velocity.0.z).length();
        
        if horizontal_speed > max_horizontal_speed {
            let scale = max_horizontal_speed / horizontal_speed;
            velocity.0.x *= scale;
            velocity.0.z *= scale;
        }
    }

    // Применяем вертикальное ускорение (гравитацию)
    if jump.is_jumping {
        velocity.0.y += GRAVITY * time.delta_secs();
        
        // Ограничиваем максимальную высоту прыжка
        if transform.translation.y > MAX_JUMP_HEIGHT {
            velocity.0.y = velocity.0.y.min(0.0);
        }
        
        // Ограничиваем максимальную скорость падения
        velocity.0.y = velocity.0.y.max(-20.0);
    }
}

// Обновляем позицию на основе скорости
fn update_position(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Jump), With<Player>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    let (mut transform, mut velocity, mut jump) = query.single_mut();
    let dt = time.delta_secs();
    
    // Применяем скорость к позиции
    transform.translation += velocity.0 * time.delta_secs();
    
    // Проверяем приземление
    if jump.is_jumping && transform.translation.y <= 0.5 {
        transform.translation.y = 0.5;
        velocity.0.y = 0.0;
        jump.is_jumping = false;
        
        // Если пробел всё ещё удерживается и таймер прыжка вышел, сразу прыгаем снова
        if keyboard_input.pressed(KeyCode::Space) && jump.jump_timer <= 0.0 {
            jump.is_jumping = true;
            velocity.0.y = JUMP_FORCE;
            jump.jump_timer = jump.jump_cooldown;
        } else if jump.jump_timer <= 0.0 {
            // Если пробел не нажат или таймер не вышел, просто разрешаем прыжок
            jump.can_jump = true;
        }
    }
    
    // Обновляем таймер прыжка
    if !jump.can_jump && jump.jump_timer > 0.0 {
        jump.jump_timer -= dt;
        if jump.jump_timer <= 0.0 {
            jump.can_jump = true;
        }
    }
    
    // Добавляем трение для горизонтального движения
    if velocity.0.length() > 0.01 {
        // Трение на земле сильнее, чем в воздухе
        let friction = if jump.is_jumping { 0.95 } else { 0.92 };
        velocity.0.x *= friction;
        velocity.0.z *= friction;
    } else {
        velocity.0.x = 0.0;
        velocity.0.z = 0.0;
    }
}

fn draw_cursor(
    camera_query: Single<(&Camera, &GlobalTransform)>,
    ground: Query<&GlobalTransform, With<Ground>>,
    windows: Single<&Window>,
    mut gizmos: Gizmos,
    mut player_query: Query<(&mut Transform, &WishDirection), With<Player>>,
) {
    let (camera, camera_transform) = *camera_query;
    let ground_transform = if let Some(transform) = ground.iter().next() {
        transform
    } else {
        return;
    };

    let Some(cursor_position) = windows.cursor_position() else {
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

    // Поворачиваем куб в направлении точки пересечения
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
            // Если нет направления к курсору, но куб движется, поворачиваем его в направлении движения
            let rotation = Quat::from_rotation_arc(forward, wish_dir.0);
            player_transform.rotation = rotation;
        }
    }
}

fn check_exit(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut app_exit_events: EventWriter<AppExit>,
) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        app_exit_events.send(AppExit::default());
    }
}

#[derive(Component)]
struct FollowCamera {
    distance: f32,       // Расстояние от камеры до куба (по оси Z)
    height: f32,         // Высота камеры над кубом
    lerp_speed: f32,     // Скорость интерполяции
}

fn follow_camera(
    time: Res<Time>,
    player_query: Query<&Transform, With<Player>>,
    mut camera_query: Query<(&mut Transform, &mut FollowCamera), (With<Camera3d>, Without<Player>)>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };
    
    for (mut camera_transform, follow) in camera_query.iter_mut() {
        // Используем только горизонтальные координаты куба,
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


