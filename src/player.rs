use bevy::prelude::*;
use crate::world::Ground;
use bevy_rapier3d::prelude::*;

#[derive(Resource)]
pub struct GameSettings {
    pub sv_maxspeed: f32,
    pub sv_accelerate: f32,
    pub sv_air_accelerate: f32,
    pub sv_gravity: f32,
    pub sv_jump_force: f32,
    pub sv_max_jump_height: f32,
    pub sv_max_air_speed: f32,
    pub sv_strafe_speed_multiplier: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            sv_maxspeed: 8.0,
            sv_accelerate: 10.0,
            sv_air_accelerate: 7.0,
            sv_gravity: -9.81,
            sv_jump_force: 5.0,
            sv_max_jump_height: 2.5,
            sv_max_air_speed: 8.0,
            sv_strafe_speed_multiplier: 1.0,
        }
    }
}

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameSettings::default())
            .add_systems(Startup, spawn_player)
            .add_systems(Update, (
                player_movement,
                apply_acceleration,
                update_position,
                player_movement_system,
            ).chain())
            .add_systems(Update, check_collision_objects);
    }
}

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

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let entity = commands.spawn(
        (
            Player,
            Jump::default(),
            Velocity::default(),
            WishDirection::default(),
            WishSpeed::default(),
        )
    ).id();
    
    commands.entity(entity).insert(Mesh3d(meshes.add(Capsule3d::new(0.25, 0.5))));
    commands.entity(entity).insert(MeshMaterial3d(materials.add(StandardMaterial {
        base_color: Color::srgb_u8(124, 144, 255),
        perceptual_roughness: 0.2,
        metallic: 0.7,
        ..default()
    })));
    commands.entity(entity).insert(Transform::from_xyz(0.0, 2.0, 0.0));
    commands.entity(entity).insert(RigidBody::Dynamic);
    commands.entity(entity).insert(Collider::capsule_y(0.25, 0.25));
    commands.entity(entity).insert(ColliderMassProperties::Density(1.0));
    commands.entity(entity).insert(Friction {
        coefficient: 0.5,
        combine_rule: CoefficientCombineRule::Average,
    });
    commands.entity(entity).insert(Restitution {
        coefficient: 0.2,
        combine_rule: CoefficientCombineRule::Average,
    });
    commands.entity(entity).insert(GravityScale(1.5));
    commands.entity(entity).insert(LockedAxes::ROTATION_LOCKED);
    commands.entity(entity).insert(ActiveEvents::COLLISION_EVENTS);
    commands.entity(entity).insert(Ccd::enabled());
}

// Считываем ввод и обновляем желаемое направление движения
fn player_movement(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    ground: Query<&GlobalTransform, With<Ground>>,
    windows: Query<&Window>,
    settings: Res<GameSettings>,
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
                
                forward_direction = (target_position - player_transform.translation).normalize_or_zero();
                
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
            direction -= right_direction * settings.sv_strafe_speed_multiplier;
        }
        
        // Движение вправо (D)
        if keyboard_input.pressed(KeyCode::KeyD) {
            direction += right_direction * settings.sv_strafe_speed_multiplier;
        }
    } else {
        // Если направление к курсору не определено, используем мировые координаты
        if keyboard_input.pressed(KeyCode::KeyW) {
            direction.z -= 1.0; // Вперед по оси Z
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
    }

    // Нормализуем направление, если оно не нулевое
    if direction.length() > 0.0 {
        // Добавляем небольшое сглаживание при изменении направления
        if wishdir.0 != Vec3::ZERO && wishdir.0 != direction {
            // Интерполируем между старым и новым направлением для более плавного поворота
            let smooth_factor = 0.7; // 70% нового направления, 30% старого
            direction = direction * smooth_factor + wishdir.0 * (1.0 - smooth_factor);
            direction = direction.normalize();
        }
        wishdir.0 = direction;
    } else {
        wishdir.0 = Vec3::ZERO;
    }
    
    // Устанавливаем желаемую скорость в зависимости от состояния прыжка
    wishspeed.0 = if jump.is_jumping { 
        settings.sv_max_air_speed 
    } else { 
        settings.sv_maxspeed 
    };
    
    // Мгновенная остановка при отпускании клавиш
    if direction.length_squared() < 0.01 && !jump.is_jumping {
        let stop_threshold = 0.5; // Порог скорости для мгновенной остановки
        if velocity.0.length_squared() < stop_threshold {
            velocity.0.x = 0.0;
            velocity.0.z = 0.0;
        }
    }
    
    // Обработка прыжка
    if keyboard_input.pressed(KeyCode::Space) && !jump.is_jumping && jump.can_jump {
        jump.is_jumping = true;
        jump.can_jump = false;
        jump.jump_timer = jump.jump_cooldown;
        velocity.0.y = settings.sv_jump_force;
    }
}

// Применяем ускорение
fn apply_acceleration(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut query: Query<(&WishDirection, &WishSpeed, &Jump, &mut Velocity)>,
) {
    let (wishdir, wishspeed, jump, mut velocity) = query.single_mut();
    let dt = time.delta_secs();
    
    if wishdir.0.length_squared() < 0.01 {
        // Если нет желаемого направления, применяем замедление
        let deceleration = if !jump.is_jumping { 5.0 } else { 2.0 }; // Уменьшено с 8.0 до 5.0
        let horizontal_vel = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
        let current_speed = horizontal_vel.length();
        
        if current_speed > 0.1 {
            // Постепенное замедление
            let drop_speed = current_speed * deceleration * dt;
            let new_speed = (current_speed - drop_speed).max(0.0);
            
            if new_speed > 0.0 {
                let scaled_vel = horizontal_vel / current_speed * new_speed;
                velocity.0.x = scaled_vel.x;
                velocity.0.z = scaled_vel.z;
            } else {
                velocity.0.x = 0.0;
                velocity.0.z = 0.0;
            }
        }
    } else {
        // Применяем горизонтальное ускорение в желаемом направлении
        let horizontal_velocity = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
        let currentspeed = horizontal_velocity.dot(wishdir.0);
        let addspeed = wishspeed.0 - currentspeed;

        if addspeed > 0.0 {
            let accel = if !jump.is_jumping { settings.sv_accelerate } else { settings.sv_air_accelerate };
            let accelspeed = (accel * dt * wishspeed.0).min(addspeed);

            velocity.0.x += wishdir.0.x * accelspeed;
            velocity.0.z += wishdir.0.z * accelspeed;
            
            // Ограничиваем горизонтальную скорость в зависимости от состояния
            let max_horizontal_speed = if jump.is_jumping { 
                settings.sv_max_air_speed 
            } else { 
                settings.sv_maxspeed
            };
            
            let horizontal_speed = Vec3::new(velocity.0.x, 0.0, velocity.0.z).length();
            
            if horizontal_speed > max_horizontal_speed {
                let scale = max_horizontal_speed / horizontal_speed;
                velocity.0.x *= scale;
                velocity.0.z *= scale;
            }
        }
    }

    // Применяем вертикальное ускорение (гравитацию) в функции update_position
    // для более точной обработки коллизий
}

// Обновляем позицию на основе скорости
pub fn update_position(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut Jump), With<Player>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    let (mut transform, mut velocity, mut jump) = query.single_mut();
    let dt = time.delta_secs();
    
    // Сглаживаем дельту времени для более стабильного движения
    let smooth_dt = dt.min(0.05);
    
    // Применяем трение покоя
    if velocity.0.length_squared() < 0.05 && !keyboard_input.pressed(KeyCode::KeyW) && 
       !keyboard_input.pressed(KeyCode::KeyA) && !keyboard_input.pressed(KeyCode::KeyS) && 
       !keyboard_input.pressed(KeyCode::KeyD) {
        velocity.0.x *= 0.8;
        velocity.0.z *= 0.8;
        
        if velocity.0.length_squared() < 0.001 {
            velocity.0.x = 0.0;
            velocity.0.z = 0.0;
        }
    } else {
        // Применяем трение скольжения
        let friction_factor = 0.97;
        if !jump.is_jumping {
            velocity.0.x *= friction_factor;
            velocity.0.z *= friction_factor;
        }
    }
    
    // Фиксированный уровень земли
    let ground_level = 0.0; // Высота над нулевым уровнем
    
    // Определяем, находится ли игрок на земле
    let grounded = transform.translation.y <= ground_level + 0.1;
    
    // Обработка прыжка - можно прыгать только если игрок на земле или близко к ней
    if keyboard_input.just_pressed(KeyCode::Space) && grounded && jump.can_jump {
        jump.is_jumping = true;
        jump.can_jump = false;
        jump.jump_timer = jump.jump_cooldown;
        velocity.0.y = settings.sv_jump_force; // Задаем начальную скорость прыжка
        
        // Диагностический вывод
        println!("Прыжок: grounded={}, can_jump={}, is_jumping={}, y_pos={}, y_vel={}", 
                grounded, jump.can_jump, jump.is_jumping, transform.translation.y, velocity.0.y);
    }
    
    // Обновляем вертикальную скорость под действием гравитации
    velocity.0.y += settings.sv_gravity * smooth_dt;
    
    // Ограничиваем максимальную высоту прыжка
    if jump.is_jumping && velocity.0.y > 0.0 && 
       transform.translation.y > ground_level + settings.sv_max_jump_height {
        velocity.0.y *= 0.5;
    }
    
    // Ограничиваем максимальную скорость падения
    if velocity.0.y < -20.0 {
        velocity.0.y = -20.0;
    }
    
    // Применяем движение плавно
    transform.translation += velocity.0 * smooth_dt;
    
    // Проверяем не опустился ли игрок ниже уровня земли
    if transform.translation.y < ground_level {
        // Фиксируем игрока на уровне земли
        transform.translation.y = ground_level;
        
        // Обнуляем вертикальную скорость и сбрасываем состояние прыжка
        velocity.0.y = 0.0;
        if jump.is_jumping {
            jump.is_jumping = false;
        }
    }
    
    // Обновляем таймер прыжка
    if !jump.can_jump && jump.jump_timer > 0.0 {
        jump.jump_timer -= smooth_dt;
        if jump.jump_timer <= 0.0 {
            jump.can_jump = true;
        }
    }
    
    // Ограничиваем максимальную горизонтальную скорость
    let max_horizontal_speed = if jump.is_jumping { 
        settings.sv_max_air_speed 
    } else { 
        settings.sv_maxspeed 
    };
    
    let horizontal_speed = Vec2::new(velocity.0.x, velocity.0.z).length();
    
    if horizontal_speed > max_horizontal_speed {
        let scale = max_horizontal_speed / horizontal_speed;
        velocity.0.x *= scale;
        velocity.0.z *= scale;
    }
}

// Система для проверки коллайдеров на некорректные позиции
fn check_collision_objects(
    mut commands: Commands,
    collider_query: Query<(Entity, &Transform, Option<&Collider>, Option<&RigidBody>)>,
) {
    // Проверяем все коллайдеры в сцене
    for (entity, transform, collider_opt, rigid_body_opt) in collider_query.iter() {
        let position = transform.translation;
        
        // Проверяем на некорректные позиции (слишком далеко или ниже допустимого уровня)
        let is_bad_position = position.y < -10.0 || position.length() > 1000.0;
        
        // Проверяем размер коллайдера, если он доступен
        let has_invalid_size = if let Some(_collider) = collider_opt {
            // Проверяем масштаб трансформации - неправильно увеличенные объекты тоже могут вызывать проблемы
            transform.scale.length() > 50.0
        } else {
            false
        };
        
        // Проверяем тип объекта - статические объекты не двигаются, поэтому не должны вызывать проблем,
        // но динамические объекты с нестандартными настройками могут вызывать проблемы
        let is_problematic_dynamic = if let Some(RigidBody::Dynamic) = rigid_body_opt {
            // Динамические объекты без коллайдера - подозрительны
            collider_opt.is_none()
        } else {
            false
        };
        
        // Если объект находится за пределами игрового мира или имеет недопустимые параметры
        if is_bad_position || has_invalid_size || is_problematic_dynamic {
            println!("Удаляем проблемный коллайдер с позицией: {:?}, размер: {:?}", 
                position, transform.scale);
            
            // Удаляем проблемный объект
            commands.entity(entity).despawn();
        }
    }
}

// Система для улучшенного управления движением игрока с плавным ускорением и замедлением
fn player_movement_system(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut player_query: Query<(&mut Jump, &mut Velocity), With<Player>>,
) {
    if let Ok((mut jump, mut velocity)) = player_query.get_single_mut() {
        // Расчет ускорения и замедления
        let acceleration = 50.0; // Ускорение при нажатии клавиш
        let deceleration = 40.0; // Замедление при отпускании клавиш
        let max_speed = settings.sv_maxspeed; // Максимальная скорость движения
        
        // Базовый вектор для движения
        let mut movement = Vec3::ZERO;
        
        // Обрабатываем ввод клавиш
        if keyboard_input.pressed(KeyCode::KeyW) {
            movement.z -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            movement.z += 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            movement.x -= 1.0;
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            movement.x += 1.0;
        }
        
        // Нормализуем вектор движения, если он не нулевой
        let movement_direction = if movement.length_squared() > 0.0 {
            movement.normalize()
        } else {
            movement
        };
        
        // Прыжок
        let grounded = !jump.is_jumping; // Используем обратную логику is_jumping для определения grounded
        
        if keyboard_input.just_pressed(KeyCode::Space) && jump.can_jump && grounded {
            jump.is_jumping = true;
            jump.can_jump = false;
            jump.jump_timer = jump.jump_cooldown;
            velocity.0.y = settings.sv_jump_force;
        }
        
        // Плавное ускорение при движении
        if movement_direction.length_squared() > 0.0 {
            // Ускоряемся в направлении ввода
            velocity.0.x += movement_direction.x * acceleration * time.delta_secs();
            velocity.0.z += movement_direction.z * acceleration * time.delta_secs();
        } else {
            // Плавное замедление при отсутствии ввода
            let current_horizontal_velocity = Vec2::new(velocity.0.x, velocity.0.z);
            if current_horizontal_velocity.length_squared() > 0.0 {
                let deceleration_factor = (deceleration * time.delta_secs()).min(1.0);
                velocity.0.x -= velocity.0.x * deceleration_factor;
                velocity.0.z -= velocity.0.z * deceleration_factor;
                
                // Останавливаем полностью при малой скорости для избежания микродвижений
                if Vec2::new(velocity.0.x, velocity.0.z).length() < 0.1 {
                    velocity.0.x = 0.0;
                    velocity.0.z = 0.0;
                }
            }
        }
        
        // Ограничиваем максимальную скорость по горизонтали
        let horizontal_velocity = Vec2::new(velocity.0.x, velocity.0.z);
        if horizontal_velocity.length() > max_speed {
            let limited_horizontal = horizontal_velocity.normalize() * max_speed;
            velocity.0.x = limited_horizontal.x;
            velocity.0.z = limited_horizontal.y;
        }
        
        // Обновляем таймер прыжка
        if jump.jump_timer > 0.0 {
            jump.jump_timer -= time.delta_secs();
            if jump.jump_timer <= 0.0 {
                jump.can_jump = true;
            }
        }
        
        // Ограничение падения 
        if velocity.0.y < -40.0 {
            velocity.0.y = -40.0;
        }
        
        // Применяем гравитацию, если в прыжке
        if jump.is_jumping {
            velocity.0.y += settings.sv_gravity * time.delta_secs();
        }
    }
}
