use bevy::prelude::*;
use crate::world::Ground;

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, (
                player_movement,
                apply_acceleration,
                update_position,
            ).chain());
    }
}

// Константы для движения
const MAX_SPEED: f32 = 8.0;
const ACCELERATE: f32 = 16.0;
const AIR_ACCELERATE: f32 = 16.0;
const GRAVITY: f32 = -9.81;
const JUMP_FORCE: f32 = 4.0;
const MAX_JUMP_HEIGHT: f32 = 2.0; // Максимальная высота прыжка
const MAX_AIR_SPEED: f32 = 16.0; // Максимальная скорость в воздухе

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Jump {
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
pub struct Velocity(Vec3);

#[derive(Component, Default)]
pub struct WishDirection(pub Vec3);

#[derive(Component, Default)]
struct WishSpeed(f32);

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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
}

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
pub fn update_position(
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
