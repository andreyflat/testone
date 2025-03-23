//! A simple 3D scene with light shining over a cube sitting on a plane.

use bevy::prelude::*;
use bevy::math::Isometry3d;

// Константы для движения
const MAX_SPEED: f32 = 4.0;
const ACCELERATE: f32 = 6.0;
const AIR_ACCELERATE: f32 = 6.0;
const GRAVITY: f32 = -9.81;
const JUMP_FORCE: f32 = 3.0;
const MAX_JUMP_HEIGHT: f32 = 2.0; // Максимальная высота прыжка
const MAX_AIR_SPEED: f32 = 16.0; // Максимальная скорость в воздухе

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, (
            player_movement,
            apply_acceleration,
            update_position,
            draw_cursor.after(update_position),
            follow_camera.after(update_position),
            display_speed.after(update_position),
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
    // ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20., 20.))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
        Ground,
    ));
    
    // cube
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(0.25, 1.0, 0.25))),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Cube,
        Jump::default(),
        Velocity::default(),
        WishDirection::default(),
        WishSpeed::default(),
    ));
    
    // light
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_translation(Vec3::ONE).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(5.0, 5.0, 7.0).looking_at(Vec3::ZERO, Vec3::Y),
        FollowCamera {
            offset: Vec3::new(5.0, 5.0, 10.0), // Смещение камеры относительно куба
            lerp_speed: 1.5, // Скорость следования за кубом
            ignore_vertical: true, // Игнорировать вертикальное движение объекта
        },
    ));
}

#[derive(Component)]
struct Cube;

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
            jump_cooldown: 0.3, // Задержка между прыжками (в секундах)
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
    mut query: Query<(&mut WishDirection, &mut WishSpeed, &mut Jump, &mut Velocity), With<Cube>>,
) {
    let (mut wishdir, mut wishspeed, mut jump, mut velocity) = query.single_mut();

    let mut direction = Vec3::ZERO;

    if keyboard_input.pressed(KeyCode::KeyW) {
        direction.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        direction.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }

    if direction.length() > 0.0 {
        direction = direction.normalize();
    }

    wishdir.0 = direction;
    wishspeed.0 = MAX_SPEED;
    
    // Прыжок - обработка только нажатия, таймер обрабатывается в update_position
    if keyboard_input.pressed(KeyCode::Space) && !jump.is_jumping && jump.can_jump {
        jump.is_jumping = true;
        jump.can_jump = false;
        jump.jump_timer = jump.jump_cooldown;
        // Устанавливаем начальную скорость прыжка здесь
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
    mut query: Query<(&mut Transform, &mut Velocity, &mut Jump), With<Cube>>,
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
        let friction = if jump.is_jumping { 0.98 } else { 0.90 };
        velocity.0.x *= friction;
        velocity.0.z *= friction;
    } else {
        velocity.0.x = 0.0;
        velocity.0.z = 0.0;
    }
}

fn draw_cursor(
    camera_query: Single<(&Camera, &GlobalTransform)>,
    ground: Single<&GlobalTransform, With<Ground>>,
    windows: Single<&Window>,
    mut gizmos: Gizmos,
    mut cube_query: Query<(&mut Transform, &WishDirection), With<Cube>>,
) {
    let (camera, camera_transform) = *camera_query;

    let Some(cursor_position) = windows.cursor_position() else {
        return;
    };

    // Вычисляем луч из камеры в мир на основе позиции курсора
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Вычисляем точку пересечения луча с плоскостью земли
    let Some(distance) = ray.intersect_plane(
        ground.translation(),
        InfinitePlane3d::new(ground.up())
    ) else {
        return;
    };
    let point = ray.get_point(distance);

    // Рисуем круг чуть выше плоскости земли в этой позиции
    gizmos.circle(
        Isometry3d::new(
            point + ground.up() * 0.01,
            Quat::from_rotation_arc(Vec3::Z, ground.up().as_vec3()),
        ),
        0.2,
        Color::WHITE,
    );

    // Поворачиваем куб в направлении точки пересечения
    if let Ok((mut cube_transform, wish_dir)) = cube_query.get_single_mut() {
        let target_point = point;
        let current_pos = cube_transform.translation;
        
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
            cube_transform.rotation = rotation;
        } else if wish_dir.0 != Vec3::ZERO {
            // Если нет направления к курсору, но куб движется, поворачиваем его в направлении движения
            let rotation = Quat::from_rotation_arc(forward, wish_dir.0);
            cube_transform.rotation = rotation;
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
    offset: Vec3,
    lerp_speed: f32,
    ignore_vertical: bool, // Игнорировать ли вертикальное движение объекта
}

fn follow_camera(
    time: Res<Time>,
    cube_query: Query<(&Transform, &Jump), With<Cube>>,
    mut camera_query: Query<(&mut Transform, &FollowCamera), (With<Camera3d>, Without<Cube>)>,
) {
    let Ok((cube_transform, jump)) = cube_query.get_single() else {
        return;
    };
    
    for (mut camera_transform, follow) in camera_query.iter_mut() {
        // Создаем позицию куба, игнорируя вертикальное положение, если нужно
        let mut target_cube_pos = cube_transform.translation;
        
        // Если нужно игнорировать вертикальное движение и куб в прыжке
        if follow.ignore_vertical && jump.is_jumping {
            // Заменяем Y-координату на базовую высоту куба (0.5)
            target_cube_pos.y = 0.5;
        }
        
        let target_position = target_cube_pos + follow.offset;
        
        // Плавно перемещаем камеру к целевой позиции
        camera_transform.translation = camera_transform.translation.lerp(
            target_position,
            follow.lerp_speed * time.delta_secs()
        );
        
        // Вычисляем целевой поворот (куб должен быть в центре вида камеры)
        let target_rotation = Quat::from_rotation_arc(
            (Vec3::NEG_Z).normalize(),
            (target_cube_pos - camera_transform.translation).normalize()
        );
        
        // Плавно поворачиваем камеру к целевой ориентации
        camera_transform.rotation = camera_transform.rotation.slerp(
            target_rotation,
            follow.lerp_speed * time.delta_secs()
        );
    }
}

// Система отображения скорости с помощью gizmos
fn display_speed(
    mut gizmos: Gizmos,
    query_cube: Query<(&Velocity, &Transform), With<Cube>>,
) {
    if let Ok((velocity, transform)) = query_cube.get_single() {
        // Получаем только горизонтальную скорость (x, z)
        let horizontal_speed = Vec3::new(velocity.0.x, 0.0, velocity.0.z).length();
        
        // Определяем цвет в зависимости от скорости
        let color = if horizontal_speed >= MAX_AIR_SPEED * 0.8 {
            // Почти максимальная скорость - красный
            Color::srgb(1.0, 0.0, 0.0)
        } else if horizontal_speed >= MAX_SPEED * 1.5 {
            // Ускоренная скорость - оранжевый
            Color::srgb(1.0, 0.5, 0.0)
        } else if horizontal_speed >= MAX_SPEED {
            // Выше нормальной скорости - желтый
            Color::srgb(1.0, 1.0, 0.0)
        } else {
            // Нормальная скорость - зеленый
            Color::srgb(0.0, 1.0, 0.0)
        };
        
        // Отображаем индикатор скорости над кубом
        let speed_scale = (horizontal_speed / MAX_SPEED).clamp(0.5, 2.0);
        let position = transform.translation + Vec3::new(0.0, 1.5, 0.0);
        
        // Рисуем индикатор скорости (круг)
        // В Bevy 0.15 gizmos.circle принимает Isometry3d вместо Vec3
        let isometry = Isometry3d::new(position, Quat::IDENTITY);
        gizmos.circle(
            isometry,
            0.2 * speed_scale,
            color,
        );
        
        // Выводим информацию о скорости в консоль (примерно раз в секунду)
        if (transform.translation.x + transform.translation.z).fract().abs() < 0.01 {
            println!("Скорость: {:.2}", horizontal_speed);
        }
    }
}
