//! A simple 3D scene with light shining over a cube sitting on a plane.

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, (
            move_cube,
            draw_cursor.after(move_cube),
            follow_camera.after(move_cube),
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
    velocity: f32,
    is_jumping: bool,
    can_jump: bool,
    jump_cooldown: f32,
    jump_timer: f32,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            velocity: 0.0,
            is_jumping: false,
            can_jump: true,
            jump_cooldown: 0.3, // Задержка между прыжками (в секундах)
            jump_timer: 0.0,
        }
    }
}

fn move_cube(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut Transform, &mut Jump), With<Cube>>,
    time: Res<Time>,
) {
    let speed = 5.0;
    let jump_force = 5.0; // Сила прыжка
    let gravity = -9.81;
    let dt = time.delta_secs();
    
    let (mut transform, mut jump) = query.single_mut();
    
    // Горизонтальное движение
    if keyboard_input.pressed(KeyCode::KeyW) {
        transform.translation.z -= speed * dt;
    }
    if keyboard_input.pressed(KeyCode::KeyS) {
        transform.translation.z += speed * dt;
    }
    if keyboard_input.pressed(KeyCode::KeyA) {
        transform.translation.x -= speed * dt;
    }
    if keyboard_input.pressed(KeyCode::KeyD) {
        transform.translation.x += speed * dt;
    }
    
    // Обработка таймера между прыжками
    if !jump.can_jump && jump.jump_timer > 0.0 {
        jump.jump_timer -= dt;
        if jump.jump_timer <= 0.0 {
            jump.can_jump = true;
        }
    }
    
    // Прыжок
    if keyboard_input.pressed(KeyCode::Space) && !jump.is_jumping && jump.can_jump {
        jump.velocity = jump_force;
        jump.is_jumping = true;
        jump.can_jump = false;
        jump.jump_timer = jump.jump_cooldown;
    }
    
    // Применяем гравитацию и обновляем позицию по Y
    if jump.is_jumping {
        jump.velocity += gravity * dt;
        transform.translation.y += jump.velocity * dt;
        
        // Проверяем приземление
        if transform.translation.y <= 0.5 {
            transform.translation.y = 0.5;
            jump.velocity = 0.0;
            jump.is_jumping = false;
            
            // Если пробел всё ещё удерживается, и таймер вышел, сразу прыгаем снова
            if keyboard_input.pressed(KeyCode::Space) && jump.jump_timer <= 0.0 {
                jump.velocity = jump_force;
                jump.is_jumping = true;
                jump.jump_timer = jump.jump_cooldown;
            } else if jump.jump_timer <= 0.0 {
                // Если пробел не нажат или таймер не вышел, просто разрешаем прыжок
                jump.can_jump = true;
            }
        }
    }
}

fn draw_cursor(
    camera_query: Single<(&Camera, &GlobalTransform)>,
    ground: Single<&GlobalTransform, With<Ground>>,
    windows: Single<&Window>,
    mut gizmos: Gizmos,
    mut cube_query: Query<&mut Transform, With<Cube>>,
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
    if let Ok(mut cube_transform) = cube_query.get_single_mut() {
        let target_point = point;
        let current_pos = cube_transform.translation;
        
        // Получаем направление только в горизонтальной плоскости
        let direction = Vec3::new(
            target_point.x - current_pos.x,
            0.0, // Игнорируем вертикальную составляющую
            target_point.z - current_pos.z,
        ).normalize();

        if direction != Vec3::ZERO {
            // Создаем поворот от вектора вперед (по умолчанию для куба) к направлению цели
            let forward = Vec3::NEG_Z; // Стандартное направление "вперед" в Bevy
            let rotation = Quat::from_rotation_arc(forward, direction);
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
    cube_query: Query<(&Transform, Option<&Jump>), With<Cube>>,
    mut camera_query: Query<(&mut Transform, &FollowCamera), (With<Camera3d>, Without<Cube>)>,
) {
    let Ok((cube_transform, maybe_jump)) = cube_query.get_single() else {
        return;
    };
    
    for (mut camera_transform, follow) in camera_query.iter_mut() {
        // Создаем позицию куба, игнорируя вертикальное положение, если нужно
        let mut target_cube_pos = cube_transform.translation;
        
        // Если нужно игнорировать вертикальное движение и куб в прыжке
        if follow.ignore_vertical && maybe_jump.is_some() && maybe_jump.unwrap().is_jumping {
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
