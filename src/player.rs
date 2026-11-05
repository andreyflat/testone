// src/player.rs - Исправленная версия с правильными коллизиями
use bevy::prelude::*;
use bevy::input::keyboard::KeyCode;
use bevy_rapier3d::prelude::*;
use crate::enemies::{ENEMY_GROUP, Health, PLAYER_GROUP, PLAYER_MAX_HEALTH};

#[derive(Resource)]
pub struct GameSettings {
    pub sv_maxspeed: f32,
    pub sv_accelerate: f32,
    pub sv_air_accelerate: f32,
    pub sv_gravity: f32,
    pub sv_jump_force: f32,
    pub sv_friction: f32,
    pub sv_stopspeed: f32,
    pub sv_air_control: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            sv_maxspeed: 8.0,
            sv_accelerate: 10.0,
            sv_air_accelerate: 7.0,
            sv_gravity: -9.81,
            sv_jump_force: 5.0,
            sv_friction: 6.0,
            sv_stopspeed: 2.5, // 100 единиц Quake 3 при 320 ед/с = 8 м/с
            sv_air_control: 120.0, // 32 * 150 из CPMA, переведено в м/с (40 единиц = 1 м)
        }
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component, Default, Debug)]
pub struct Velocity(pub Vec3);

#[derive(Component, Default, Debug)]
pub struct WishDirection(pub Vec3);

#[derive(Component, Default, Debug)]
pub struct WishSpeed(pub f32);

#[derive(Component)]
pub struct Jump {
    pub is_jumping: bool,
    pub can_jump: bool,
    pub jump_timer: f32,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            is_jumping: false,
            can_jump: true,
            jump_timer: 0.0,
        }
    }
}

#[derive(Component)]
pub struct PlayerCamera;

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    jump: Jump,
    velocity: Velocity,
    wish_direction: WishDirection,
    wish_speed: WishSpeed,
    transform: Transform,
    rigid_body: RigidBody,
    collider: Collider,
    friction: Friction,
    restitution: Restitution,
    gravity_scale: GravityScale,
    locked_axes: LockedAxes,
    damping: Damping,
    mesh: Mesh3d,
    material: MeshMaterial3d<StandardMaterial>,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameSettings::default())
            .add_systems(Startup, spawn_player)
            .add_systems(Update, (
                (
                    handle_input,
                    apply_acceleration_cpma,
                    move_kinematic_player_by_velocity,
                ).chain(),
                debug_player_position,
                emergency_respawn,
            ));
    }
}

pub fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let player = commands
        .spawn(PlayerBundle {
            player: Player,
            jump: Jump::default(),
            velocity: Velocity::default(),
            wish_direction: WishDirection::default(),
            wish_speed: WishSpeed::default(),
            // ВАЖНО: Поднимаем игрока значительно выше пола
            transform: Transform::from_xyz(0.0, 3.0, 0.0), // Было 1.0, стало 3.0
            rigid_body: RigidBody::KinematicPositionBased,
            // Уменьшаем коллайдер для лучшей совместимости
            collider: Collider::capsule_y(0.5, 0.3), // высота 0.5, радиус 0.3
            friction: Friction::coefficient(0.0), // Убираем трение для Quake-физики
            restitution: Restitution::coefficient(0.0), // Убираем отскок
            gravity_scale: GravityScale(0.0), // Отключаем встроенную гравитацию, используем свою
            locked_axes: LockedAxes::ROTATION_LOCKED,
            damping: Damping {
                linear_damping: 0.0, // Убираем демпфирование
                angular_damping: 1.0,
            },
            // Визуальная капсула - больше коллайдера для лучшей видимости
            mesh: Mesh3d(meshes.add(Capsule3d::new(0.3, 1.0))), // немного меньше
            material: MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.1, 0.3, 1.0), // Яркий синий
                emissive: LinearRgba::rgb(0.1, 0.1, 0.3), // Добавляем свечение
                metallic: 0.0,
                perceptual_roughness: 0.5,
                ..default()
            })),
        })
        .id();

    let groups = CollisionGroups::new(PLAYER_GROUP, Group::ALL ^ ENEMY_GROUP);
    commands.entity(player)
        .insert(Health::new(PLAYER_MAX_HEALTH))
        .insert(groups)
        .insert(KinematicCharacterController {
            filter_groups: Some(groups), // Контроллер не упирается во врагов
            offset: CharacterLength::Absolute(0.02), // Увеличили offset
            up: Vec3::Y,
            max_slope_climb_angle: 0.785398, // 45 градусов
            min_slope_slide_angle: 0.785398,
            slide: true, // Без этого контроллер останавливается при первом касании пола
            apply_impulse_to_dynamic_bodies: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.3),
                min_width: CharacterLength::Absolute(0.05),
                include_dynamic_bodies: false,
            }),
            ..default()
        });
}

// Система отладки позиции игрока
fn debug_player_position(
    player_query: Query<(&Transform, &Velocity), With<Player>>,
) {
    if let Ok((transform, velocity)) = player_query.single() {
        if transform.translation.y < -5.0 {
            println!("ПРЕДУПРЕЖДЕНИЕ: Игрок провалился! Позиция: {:?}, Скорость: {:?}", 
                transform.translation, velocity.0);
        }
    }
}

pub fn apply_acceleration_cpma(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut query: Query<(
        &mut Velocity,
        &WishDirection,
        &WishSpeed,
        &mut Jump,
        &Transform,
        Option<&KinematicCharacterControllerOutput>, // Делаем опциональным
    ), With<Player>>,
) {
    for (mut velocity, wish_dir, wish_speed, mut jump, transform, kcc_output) in query.iter_mut() {
        let dt = time.delta_secs();
        
        // Проверяем, находимся ли мы на земле
        let on_ground = if let Some(kcc) = kcc_output {
            kcc.grounded
        } else {
            // Fallback: проверяем высоту над землей
            transform.translation.y <= 1.1 // Немного выше коллайдера пола
        };

        let mut vel = velocity.0;
        let wish_dir = wish_dir.0.normalize_or_zero();
        let wish_speed = wish_speed.0;

        // Трение как в Quake 3. При зажатом прыжке не применяется,
        // иначе распрыжка теряла бы скорость в момент приземления.
        if on_ground && !jump.is_jumping {
            let speed = Vec3::new(vel.x, 0.0, vel.z).length();
            if speed > 0.0 {
                let control = speed.max(settings.sv_stopspeed);
                let new_speed = (speed - control * settings.sv_friction * dt).max(0.0);
                let scale = new_speed / speed;
                vel.x *= scale;
                vel.z *= scale;
            }
        }

        // Применяем ускорение только если есть желаемое направление
        if wish_dir != Vec3::ZERO {
            let accel = if on_ground {
                settings.sv_accelerate
            } else {
                settings.sv_air_accelerate
            };

            let current_speed = vel.dot(wish_dir);
            let add_speed = wish_speed - current_speed;

            if add_speed > 0.0 {
                let max_accel = accel * wish_speed * dt;
                let accel_speed = max_accel.min(add_speed);
                vel += accel_speed * wish_dir;
            }

            // Air control как в CPMA: поворачивает скорость к нажатому направлению,
            // не меняя её величину. Мышь при виде сверху не поворачивает движение,
            // поэтому работает для любого направления, а не только для чистого W.
            if !on_ground {
                let horizontal = Vec3::new(vel.x, 0.0, vel.z);
                let speed = horizontal.length();
                let dot = horizontal.normalize_or_zero().dot(wish_dir);
                if dot > 0.0 {
                    let k = settings.sv_air_control * dot * dot * dt;
                    let turned = (horizontal + wish_dir * k).normalize_or_zero() * speed;
                    vel.x = turned.x;
                    vel.z = turned.z;
                }
            }
        }

        // Применяем собственную гравитацию
        if !on_ground {
            vel.y += settings.sv_gravity * dt;
        } else if vel.y < 0.0 {
            // Приземлились: скорость падения иначе вдавливает капсулу в пол
            vel.y = 0.0;
        }

        // Обработка прыжка
        if on_ground {
            if jump.is_jumping && jump.can_jump {
                vel.y = settings.sv_jump_force;
                jump.can_jump = false;
                jump.jump_timer = 0.0;
            } else {
                jump.can_jump = true;
            }
        } else {
            jump.jump_timer += dt;
        }

        // Ограничиваем падение
        vel.y = vel.y.max(-50.0);

        // Гарантия, что скорость корректна
        if !vel.is_finite() || vel.length_squared() > 10_000.0 {
            vel = Vec3::ZERO;
        }

        velocity.0 = vel;
    }
}

pub fn move_kinematic_player_by_velocity(
    time: Res<Time>,
    mut query: Query<(
        &Velocity,
        &mut KinematicCharacterController,
    ), With<Player>>,
) {
    for (velocity, mut controller) in query.iter_mut() {
        let delta = velocity.0 * time.delta_secs();
        
        if delta.is_finite() && delta.length_squared() < 100.0 {
            controller.translation = Some(delta);
        } else {
            controller.translation = Some(Vec3::ZERO);
        }
    }
}

pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    settings: Res<GameSettings>,
    mut query: Query<(&mut WishDirection, &mut WishSpeed, &mut Jump), With<Player>>,
) {
    for (mut wish_dir, mut wish_speed, mut jump) in query.iter_mut() {
        let mut direction = Vec3::ZERO;

        // Стандартное WASD управление
        if keyboard.pressed(KeyCode::KeyW) {
            direction.z -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            direction.z += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            direction.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            direction.x += 1.0;
        }

        // Прыжок: зажатый пробел отталкивает сразу после приземления
        jump.is_jumping = keyboard.pressed(KeyCode::Space);

        wish_dir.0 = direction.normalize_or_zero();
        wish_speed.0 = if direction.length_squared() > 0.0 { settings.sv_maxspeed } else { 0.0 };
    }
}

pub fn emergency_respawn(
    mut player_query: Query<(&mut Transform, &mut Velocity), With<Player>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if let Ok((mut transform, mut velocity)) = player_query.single_mut() {
        // Автоматическое восстановление если игрок провалился
        if transform.translation.y < -2.0 {
            println!("Игрок провалился! Восстанавливаем позицию...");
            transform.translation = Vec3::new(0.0, 3.0, 0.0);
            velocity.0 = Vec3::ZERO;
        }
        
        // Ручное восстановление на R
        if keyboard.just_pressed(KeyCode::KeyR) {
            println!("Ручное восстановление позиции игрока");
            transform.translation = Vec3::new(0.0, 3.0, 0.0);
            velocity.0 = Vec3::ZERO;
        }
    }
}
