// src/enemies.rs - Враги, бесконечные волны, урон при касании и смерть игрока
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use crate::lights::raytraced;
use crate::player::Player;
use crate::weapons::Bullet;
use crate::world::{ARENA_HALF_SIZE, is_blocked};

pub const PLAYER_MAX_HEALTH: f32 = 100.0;
// Игрок и враги не сталкиваются физически: враг наносит урон при касании, но не служит стеной
pub const PLAYER_GROUP: Group = Group::GROUP_1;
pub const ENEMY_GROUP: Group = Group::GROUP_2;

/// Высота центра врага над началом координат. На ней же летят пули.
pub const ENEMY_CENTER_Y: f32 = 0.6;
const ENEMY_HALF_SIZE: f32 = 0.5;
/// Верх низкого врага и граница между этажами двухэтажного.
const LOWER_FLOOR_TOP_Y: f32 = ENEMY_CENTER_Y + ENEMY_HALF_SIZE;
const UPPER_FLOOR_TOP_Y: f32 = LOWER_FLOOR_TOP_Y + ENEMY_HALF_SIZE * 2.0;
const ARMOR_HEALTH: f32 = 75.0;
const TALL_ENEMY_SPEED_FACTOR: f32 = 0.7;

// Обход препятствий: враг, который почти не сдвинулся за время проверки, шагает вбок
const STUCK_CHECK_INTERVAL: f32 = 0.5;
const STUCK_MOVED_FRACTION: f32 = 0.25;
const SIDESTEP_DURATION: f32 = 1.0;
const NEAR_PLAYER_DISTANCE: f32 = 1.5;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HALF_HEIGHT: f32 = 0.8;
const CONTACT_DAMAGE: f32 = 10.0;
const CONTACT_COOLDOWN: f32 = 0.5;

const WAVE_SPAWN_RADIUS: f32 = 15.0;
const WAVE_DELAY: f32 = 2.0;

pub struct EnemiesPlugin;
impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaveState>()
            .add_systems(Update, (
                wave_system,
                enemy_ai_system,
                bullet_collision,
                enemy_death_system,
                enemy_contact_damage,
                player_death_system,
            ).chain());
    }
}

#[derive(Resource)]
pub struct WaveState {
    pub wave: u32,
    pub delay: f32,
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            wave: 0,
            delay: WAVE_DELAY,
        }
    }
}

#[derive(Component)]
pub struct Enemy {
    pub speed: f32,
    pub attack_cooldown: f32,
    pub stuck_timer: f32,
    pub last_position: Vec3,
    pub sidestep_timer: f32,
    pub sidestep_side: f32,
}

/// Верхний этаж двухэтажного врага. Пока броня цела, тело неуязвимо.
#[derive(Component)]
pub struct Armor {
    pub current: f32,
    pub plate: Entity,
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
}

impl Health {
    pub fn new(max_health: f32) -> Self {
        Self {
            current: max_health,
        }
    }

    pub fn take_damage(&mut self, damage: f32) {
        self.current = (self.current - damage).max(0.0);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

/// Когда врагов не осталось, после паузы запускает следующую, более сложную волну.
fn wave_system(
    time: Res<Time>,
    mut wave_state: ResMut<WaveState>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    enemies: Query<(), With<Enemy>>,
    player: Query<&Transform, With<Player>>,
) {
    if !enemies.is_empty() {
        return;
    }
    wave_state.delay -= time.delta_secs();
    if wave_state.delay > 0.0 {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };

    wave_state.wave += 1;
    wave_state.delay = WAVE_DELAY;
    let wave = wave_state.wave;
    let count = 1 + wave * 2;
    let tall_count = wave - 1; // С каждой следующей волной на один двухэтажный больше
    let speed = (2.5 + wave as f32 * 0.25).min(7.0);
    info!("Волна {wave}: низких {count}, двухэтажных {tall_count}, скорость {speed:.2}");

    // Золотой угол равномерно раскладывает врагов по кольцу вокруг игрока
    const GOLDEN_ANGLE: f32 = 2.399_963;
    for i in 0..count + tall_count {
        let position = spawn_point(player.translation, (i + wave) as f32 * GOLDEN_ANGLE);
        if i < count {
            commands.spawn(enemy_body(
                &mut meshes,
                &mut materials,
                position,
                speed,
                Color::srgb(0.8, 0.2, 0.2),
            ));
        } else {
            spawn_tall_enemy(&mut commands, &mut meshes, &mut materials, position, speed);
        }
    }
}

/// Точка на кольце вокруг игрока, не занятая стенами и платформами.
/// Если угол упирается в геометрию, поворачивает дальше и сужает кольцо.
fn spawn_point(player: Vec3, angle: f32) -> Vec3 {
    // Отступ с запасом на диагональ куба двухэтажного врага
    const MARGIN: f32 = ENEMY_HALF_SIZE * 1.5;
    let limit = ARENA_HALF_SIZE - MARGIN;
    let candidate = |angle: f32, radius: f32| Vec3::new(
        (player.x + angle.cos() * radius).clamp(-limit, limit),
        ENEMY_CENTER_Y,
        (player.z + angle.sin() * radius).clamp(-limit, limit),
    );
    for radius in [WAVE_SPAWN_RADIUS, WAVE_SPAWN_RADIUS * 0.75, WAVE_SPAWN_RADIUS * 0.5] {
        for step in 0..12 {
            let position = candidate(angle + step as f32 * 0.3, radius);
            if !is_blocked(position, MARGIN) {
                return position;
            }
        }
    }
    candidate(angle, WAVE_SPAWN_RADIUS)
}

/// Низкий враг и нижний этаж двухэтажного.
fn enemy_body(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    speed: f32,
    color: Color,
) -> impl Bundle {
    let mesh = meshes.add(raytraced(Cuboid::new(1.0, 1.0, 1.0)));
    (
        Mesh3d(mesh.clone()),
        RaytracingMesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: color,
            ..default()
        })),
        Transform::from_translation(position),

        // Физика
        RigidBody::Dynamic,
        Velocity::default(),
        GravityScale(0.0), // Враги движутся строго в плоскости арены
        Collider::cuboid(ENEMY_HALF_SIZE, ENEMY_HALF_SIZE, ENEMY_HALF_SIZE),
        CollisionGroups::new(ENEMY_GROUP, Group::ALL ^ PLAYER_GROUP),
        LockedAxes::ROTATION_LOCKED, // Не позволяем врагу вращаться

        // Компоненты врага
        Enemy {
            speed,
            attack_cooldown: 0.0,
            stuck_timer: STUCK_CHECK_INTERVAL,
            last_position: position,
            sidestep_timer: 0.0,
            sidestep_side: 1.0,
        },
        Health::new(50.0),

        Name::new("Enemy"),
    )
}

/// Двухэтажный враг: тело снизу и броня сверху. Броню достают только пули, выпущенные в прыжке.
fn spawn_tall_enemy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    position: Vec3,
    speed: f32,
) {
    let plate_mesh = meshes.add(raytraced(Cuboid::new(1.0, 1.0, 1.0)));
    let plate = commands.spawn((
        Mesh3d(plate_mesh.clone()),
        RaytracingMesh3d(plate_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.75, 0.8), // Стальная броня
            ..default()
        })),
        Transform::from_xyz(0.0, ENEMY_HALF_SIZE * 2.0, 0.0),
    )).id();

    let half = ENEMY_HALF_SIZE;
    commands
        .spawn(enemy_body(
            meshes,
            materials,
            position,
            speed * TALL_ENEMY_SPEED_FACTOR,
            Color::srgb(0.45, 0.1, 0.1),
        ))
        .insert((
            Collider::compound(vec![
                (Vec3::ZERO, Quat::IDENTITY, Collider::cuboid(half, half, half)),
                (Vec3::Y * half * 2.0, Quat::IDENTITY, Collider::cuboid(half, half, half)),
            ]),
            Armor {
                current: ARMOR_HEALTH,
                plate,
            },
        ))
        .add_child(plate);
}

fn enemy_ai_system(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    mut enemies: Query<(&Transform, &mut Enemy, &mut Velocity), Without<Player>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let dt = time.delta_secs();

    for (transform, mut enemy, mut velocity) in enemies.iter_mut() {
        let offset = player.translation - transform.translation;
        let flat_offset = Vec3::new(offset.x, 0.0, offset.z);
        let direction = flat_offset.normalize_or_zero();

        enemy.stuck_timer -= dt;
        if enemy.stuck_timer <= 0.0 {
            let moved = transform.translation.distance(enemy.last_position);
            let expected = enemy.speed * STUCK_CHECK_INTERVAL;
            if moved < expected * STUCK_MOVED_FRACTION
                && flat_offset.length() > NEAR_PLAYER_DISTANCE
                && enemy.sidestep_timer <= 0.0
            {
                enemy.sidestep_timer = SIDESTEP_DURATION;
                enemy.sidestep_side = -enemy.sidestep_side;
            }
            enemy.stuck_timer = STUCK_CHECK_INTERVAL;
            enemy.last_position = transform.translation;
        }

        let heading = if enemy.sidestep_timer > 0.0 {
            enemy.sidestep_timer -= dt;
            Vec3::new(-direction.z, 0.0, direction.x) * enemy.sidestep_side
        } else {
            direction
        };
        velocity.linear = heading * enemy.speed;
    }
}

/// Пуля исчезает при любом касании, кроме самого игрока. Урон получают только враги.
fn bullet_collision(
    mut commands: Commands,
    mut collision_events: MessageReader<CollisionEvent>,
    bullets: Query<(&Bullet, &Transform)>,
    player: Query<(), With<Player>>,
    mut enemies: Query<(&mut Health, Option<&mut Armor>), With<Enemy>>,
    mut spent_bullets: Local<Vec<Entity>>,
) {
    spent_bullets.clear();
    for collision_event in collision_events.read() {
        let CollisionEvent::Started(entity1, entity2, _) = collision_event else {
            continue;
        };
        let (bullet_entity, other) = if bullets.contains(*entity1) {
            (*entity1, *entity2)
        } else if bullets.contains(*entity2) {
            (*entity2, *entity1)
        } else {
            continue;
        };

        // Одна пуля попадает только один раз, даже если задела двоих за кадр
        if player.contains(other) || spent_bullets.contains(&bullet_entity) {
            continue;
        }
        spent_bullets.push(bullet_entity);
        commands.entity(bullet_entity).try_despawn();

        let (Ok((bullet, bullet_transform)), Ok((mut health, armor))) =
            (bullets.get(bullet_entity), enemies.get_mut(other))
        else {
            continue;
        };

        match armor {
            Some(mut armor) if armor.current > 0.0 => {
                // Пуля с земли упирается в тело под защитой брони и не наносит урона
                if bullet_transform.translation.y < LOWER_FLOOR_TOP_Y {
                    continue;
                }
                armor.current -= bullet.damage;
                if armor.current <= 0.0 {
                    let half = ENEMY_HALF_SIZE;
                    commands.entity(armor.plate).try_despawn();
                    commands
                        .entity(other)
                        .remove::<Armor>()
                        .insert(Collider::cuboid(half, half, half));
                    info!("Броня сбита");
                }
            }
            _ => health.take_damage(bullet.damage),
        }
    }
}

fn enemy_death_system(
    mut commands: Commands,
    enemies: Query<(Entity, &Health), With<Enemy>>,
) {
    for (entity, health) in enemies.iter() {
        if health.is_dead() {
            commands.entity(entity).despawn();
        }
    }
}

/// Урон считается по пересечению объёмов, а не по физике: над врагом можно перепрыгнуть.
fn enemy_contact_damage(
    time: Res<Time>,
    mut player: Query<(&Transform, &mut Health), With<Player>>,
    mut enemies: Query<(&Transform, &mut Enemy, Has<Armor>)>,
) {
    let Ok((player_transform, mut health)) = player.single_mut() else {
        return;
    };

    for (transform, mut enemy, has_armor) in enemies.iter_mut() {
        enemy.attack_cooldown = (enemy.attack_cooldown - time.delta_secs()).max(0.0);

        let offset = player_transform.translation - transform.translation;
        let reach = ENEMY_HALF_SIZE + PLAYER_RADIUS;
        let touching = offset.x.abs() < reach
            && offset.z.abs() < reach
            && player_transform.translation.y - PLAYER_HALF_HEIGHT
                < if has_armor { UPPER_FLOOR_TOP_Y } else { LOWER_FLOOR_TOP_Y };

        if touching && enemy.attack_cooldown <= 0.0 {
            enemy.attack_cooldown = CONTACT_COOLDOWN;
            health.take_damage(CONTACT_DAMAGE);
            info!("Игрок получил урон, здоровье: {}", health.current);
        }
    }
}

/// Смерть запускает забег заново с первой волны.
fn player_death_system(
    mut commands: Commands,
    mut player: Query<(&mut Transform, &mut crate::player::Velocity, &mut Health), With<Player>>,
    enemies: Query<Entity, With<Enemy>>,
    mut wave_state: ResMut<WaveState>,
) {
    let Ok((mut transform, mut velocity, mut health)) = player.single_mut() else {
        return;
    };
    if !health.is_dead() {
        return;
    }

    info!("Игрок погиб на волне {}. Новый забег", wave_state.wave);
    for entity in enemies.iter() {
        commands.entity(entity).despawn();
    }
    transform.translation = Vec3::new(0.0, 3.0, 0.0);
    velocity.0 = Vec3::ZERO;
    health.current = PLAYER_MAX_HEALTH;
    *wave_state = WaveState::default();
}
