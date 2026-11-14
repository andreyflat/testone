// src/player.rs - Исправленная версия с правильными коллизиями
use bevy::prelude::*;
use bevy::input::keyboard::KeyCode;
use bevy_rapier3d::prelude::*;
use crate::enemies::{ENEMY_GROUP, Health, PLAYER_GROUP, PLAYER_MAX_HEALTH};
use crate::world::ARENA_HALF_SIZE;

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
    /// Вертикальная скорость прыжка от стены. Больше обычного прыжка, чтобы подниматься выше.
    pub sv_wall_jump_force: f32,
    /// Скорость, с которой прыжок отталкивает от стены.
    pub sv_wall_jump_push: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            sv_maxspeed: 8.0,
            sv_accelerate: 10.0,
            sv_air_accelerate: 7.0,
            sv_gravity: -20.0, // 800 ед/с² из Quake 3 (40 единиц = 1 м)
            sv_jump_force: 6.75, // 270 ед/с: высота ~1.14 м, ~0.68 с в воздухе
            sv_friction: 6.0,
            sv_stopspeed: 2.5, // 100 единиц Quake 3 при 320 ед/с = 8 м/с
            sv_air_control: 120.0, // 32 * 150 из CPMA, переведено в м/с (40 единиц = 1 м)
            sv_wall_jump_force: 9.0, // Подъём ~2 м: хватает, чтобы забраться на перегородку
            sv_wall_jump_push: 4.5,
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
    /// Сколько ещё секунд действует нажатие прыжка для отскока от стены.
    /// Позволяет нажать чуть раньше касания стены.
    pub wall_jump_buffer: f32,
    /// Нормаль стены последнего отскока. От той же стены второй раз до приземления не оттолкнуться,
    /// иначе по одной стене можно было бы взбираться бесконечно.
    pub last_wall_normal: Option<Vec3>,
}

impl Default for Jump {
    fn default() -> Self {
        Self {
            is_jumping: false,
            can_jump: true,
            jump_timer: 0.0,
            wall_jump_buffer: 0.0,
            last_wall_normal: None,
        }
    }
}

#[derive(Component)]
pub struct PlayerCamera;

const PLAYER_RADIUS: f32 = 0.3;
/// На каком расстоянии от капсулы стена ещё считается касанием для отскока.
const WALL_JUMP_REACH: f32 = 0.25;
const WALL_JUMP_BUFFER: f32 = 0.15;
/// Косинус угла между стенами, начиная с которого стена считается той же самой.
const SAME_WALL_DOT: f32 = 0.7;

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
            collider: Collider::capsule_y(0.5, PLAYER_RADIUS), // высота 0.5, радиус 0.3
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
                // Силуэт в темноте. Меш не участвует в трассировке, поэтому не тратит сэмплы света
                emissive: LinearRgba::rgb(10.0, 10.0, 30.0),
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

/// Ближайшая вертикальная неподвижная стена рядом с капсулой: её нормаль, направленная от стены.
fn find_wall(rapier: &RapierContext, position: Vec3) -> Option<Vec3> {
    const DIRECTIONS: usize = 8;
    let reach = PLAYER_RADIUS + WALL_JUMP_REACH;
    let mut best: Option<(f32, Vec3)> = None;
    // Два уровня лучей: низкие перегородки и ящики не попадут в луч на высоте центра
    for height in [0.0, -0.4] {
        let origin = position + Vec3::Y * height;
        for i in 0..DIRECTIONS {
            let angle = i as f32 * std::f32::consts::TAU / DIRECTIONS as f32;
            let dir = Vec3::new(angle.cos(), 0.0, angle.sin());
            let Some((_, hit)) =
                rapier.cast_ray_and_get_normal(origin, dir, reach, false, QueryFilter::only_fixed())
            else {
                continue;
            };
            let normal = hit.normal;
            // Пол и потолок для отскока не годятся
            if normal.y.abs() > 0.3 {
                continue;
            }
            if best.is_none_or(|(toi, _)| hit.time_of_impact < toi) {
                best = Some((hit.time_of_impact, Vec3::new(normal.x, 0.0, normal.z).normalize()));
            }
        }
    }
    best.map(|(_, normal)| normal)
}

pub fn apply_acceleration_cpma(
    time: Res<Time>,
    settings: Res<GameSettings>,
    rapier: ReadRapierContext,
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

        // Прыжок от стены: в воздухе у стены, от которой ещё не отталкивались с последнего приземления
        jump.wall_jump_buffer = (jump.wall_jump_buffer - dt).max(0.0);
        if on_ground {
            // Нажатие с земли - обычный прыжок, он не должен сразу же сработать как отскок от стены
            jump.wall_jump_buffer = 0.0;
            jump.last_wall_normal = None;
        } else if jump.wall_jump_buffer > 0.0
            && let Some(normal) = rapier.single().ok().and_then(|r| find_wall(&r, transform.translation))
            && jump.last_wall_normal.is_none_or(|last| last.dot(normal) < SAME_WALL_DOT)
        {
            // Гасим движение в стену и отталкиваемся от неё, скорость вдоль стены сохраняется
            let mut horizontal = Vec3::new(vel.x, 0.0, vel.z);
            let into_wall = horizontal.dot(normal);
            if into_wall < 0.0 {
                horizontal -= normal * into_wall;
            }
            horizontal += normal * settings.sv_wall_jump_push;
            vel = Vec3::new(horizontal.x, vel.y.max(settings.sv_wall_jump_force), horizontal.z);
            jump.wall_jump_buffer = 0.0;
            jump.last_wall_normal = Some(normal);
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
        &Transform,
        &mut Velocity,
        &mut KinematicCharacterController,
    ), With<Player>>,
) {
    // Стены периметра невысокие, поэтому за край арены не пускает это ограничение
    let limit = ARENA_HALF_SIZE - PLAYER_RADIUS;
    for (transform, mut velocity, mut controller) in query.iter_mut() {
        let mut delta = velocity.0 * time.delta_secs();
        let position = transform.translation;
        for axis in [0, 2] {
            let next = position[axis] + delta[axis];
            if next.abs() > limit {
                delta[axis] = next.clamp(-limit, limit) - position[axis];
                // Скорость в сторону края гасится, как при ударе о стену
                if velocity.0[axis] * next > 0.0 {
                    velocity.0[axis] = 0.0;
                }
            }
        }

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
        if keyboard.just_pressed(KeyCode::Space) {
            jump.wall_jump_buffer = WALL_JUMP_BUFFER;
        }

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
