use bevy::prelude::*;
use bevy::input::keyboard::KeyCode;
use bevy_rapier3d::prelude::*;

#[derive(Resource)]
pub struct GameSettings {
    pub sv_maxspeed: f32,
    pub sv_accelerate: f32,
    pub sv_air_accelerate: f32,
    pub sv_gravity: f32,
    pub sv_jump_force: f32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            sv_maxspeed: 8.0,
            sv_accelerate: 10.0,
            sv_air_accelerate: 7.0,
            sv_gravity: -9.81,
            sv_jump_force: 5.0,
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

#[derive(Component, Default, Debug)]
#[allow(dead_code)]
pub struct Visibility(pub bool);

#[derive(Component, Default, Debug)]
#[allow(dead_code)]
pub struct InheritedVisibility(pub bool);

#[derive(Component, Default, Debug)]
#[allow(dead_code)]
pub struct ViewVisibility(pub bool);

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,
    jump: Jump,
    velocity: Velocity,
    wish_direction: WishDirection,
    wish_speed: WishSpeed,
    transform: Transform,
    visibility: Visibility,
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
                handle_input,
                apply_acceleration_cpma,
                move_kinematic_player_by_velocity,
            ).chain());
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
            transform: Transform::from_xyz(0.0, 1.0, 0.0),
            visibility: Visibility::default(),
            rigid_body: RigidBody::KinematicPositionBased,
            collider: Collider::capsule_y(0.25, 0.25),
            friction: Friction::coefficient(0.7),
            restitution: Restitution::coefficient(0.3),
            gravity_scale: GravityScale(1.0),
            locked_axes: LockedAxes::ROTATION_LOCKED,
            damping: Damping {
                linear_damping: 0.5,
                angular_damping: 1.0,
            },
            mesh: Mesh3d(meshes.add(Capsule3d::new(0.25, 0.5))),
            material: MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.0, 0.0, 1.0),
                ..default()
            })),
        })
        .id();

    commands.entity(player)
        .insert(ActiveEvents::COLLISION_EVENTS)
        .insert(KinematicCharacterController {
            offset: CharacterLength::Absolute(0.01),
            up: Vec3::Y,
            max_slope_climb_angle: 0.785398, // 45 градусов
            min_slope_slide_angle: 0.785398, // 45 градусов
            slide: false,
            apply_impulse_to_dynamic_bodies: true,
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.5),
                min_width: CharacterLength::Absolute(0.1),
                include_dynamic_bodies: false,
            }),
            ..default()
        });
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
        &KinematicCharacterControllerOutput,
    ), With<Player>>,
) {
    for (mut velocity, wish_dir, wish_speed, mut jump, _transform, kcc_output) in query.iter_mut() {
        let dt = time.delta_secs();
        let on_ground = kcc_output.grounded;

        let mut vel = velocity.0;
        let wish_dir = wish_dir.0.normalize_or_zero();
        let wish_speed = wish_speed.0;

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
        }

        // Ограничиваем горизонтальную скорость
        if on_ground {
            let horizontal_speed = Vec3::new(vel.x, 0.0, vel.z).length();
            if horizontal_speed > settings.sv_maxspeed {
                let scale = settings.sv_maxspeed / horizontal_speed;
                vel.x *= scale;
                vel.z *= scale;
            }
        }

        // Применяем гравитацию
        vel.y += settings.sv_gravity * dt;

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

        // Гарантия, что скорость не NaN и не бесконечная
        if !vel.is_finite() || vel.length_squared() > 1_000_000.0 {
            vel = Vec3::ZERO;
        }

        velocity.0 = vel;
    }
}


pub fn move_kinematic_player_by_velocity(
    time: Res<Time>,
    mut query: Query<(
        &Velocity,
        &mut Transform,
        &mut KinematicCharacterController,
    ), With<Player>>,
) {
    for (velocity, mut transform, mut controller) in query.iter_mut() {
        let delta = velocity.0 * time.delta_secs();
        if delta.is_finite() {
            let target = transform.translation + delta;
            if target.is_finite() && target.length_squared() < 10_000.0 * 10_000.0 {
                controller.translation = Some(delta);
                // Обновляем позицию только если нет коллизии
                if let Some(offset) = controller.translation {
                    transform.translation += offset;
                }
            }
        }
    }
}

pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<(&mut WishDirection, &mut WishSpeed, &mut Jump), With<Player>>,
) {
    for (mut wish_dir, mut wish_speed, mut jump) in query.iter_mut() {
        let mut direction = Vec3::ZERO;

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

        if keyboard.just_pressed(KeyCode::Space) {
            jump.is_jumping = true;
        } else {
            jump.is_jumping = false;
        }

        wish_dir.0 = direction.normalize_or_zero();
        wish_speed.0 = if direction.length_squared() > 0.0 { 8.0 } else { 0.0 };
    }
}
