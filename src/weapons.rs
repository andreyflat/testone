// src/weapons.rs - Оружие игрока: перчатка, автомат, рейлган и ракетница
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use crate::enemies::{Armor, ENEMY_GROUP, ENEMY_RADIUS, Enemy, Health, damage_enemy, enemy_top_y};
use crate::lights::raytraced;
use crate::player::{Player, PlayerCamera};

/// Выстрел идёт ниже центра игрока: стоя на полу, он попадает точно в центр низкого врага.
/// В прыжке выстрел поднимается вместе с игроком и может пройти над низкими врагами.
const MUZZLE_OFFSET_Y: f32 = -0.32;
/// Близко к игроку, иначе пуля появляется уже за врагом, стоящим вплотную.
const MUZZLE_DISTANCE: f32 = 0.4;
const PLAYER_RADIUS: f32 = 0.3;

const GAUNTLET_DAMAGE: f32 = 50.0;
/// Расстояние от поверхности игрока до поверхности врага, на котором достаёт удар.
const GAUNTLET_REACH: f32 = 0.6;
/// Косинус половины угла перед игроком, в котором действует удар.
const GAUNTLET_CONE_COS: f32 = 0.5;

const MACHINE_GUN_DAMAGE: f32 = 5.0;
const BULLET_SPEED: f32 = 50.0;
const BULLET_LIFETIME: f32 = 2.0;

const RAIL_DAMAGE: f32 = 100.0;
const RAIL_RANGE: f32 = 80.0;
const RAIL_TRAIL_DURATION: f32 = 0.5;
const RAIL_TRAIL_RADIUS: f32 = 0.03;

const ROCKET_DAMAGE: f32 = 100.0;
/// Скорость ракеты: медленная, от неё можно увернуться.
const ROCKET_SPEED: f32 = 10.0;
const ROCKET_SPLASH_RADIUS: f32 = 1.5;
const ROCKET_LIFETIME: f32 = 6.0;

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimPoint>()
            .add_systems(Startup, setup_weapon_assets)
            .add_systems(Update, (
                (select_weapon, aim_player, handle_shooting).chain(),
                update_bullets,
                fade_effects,
            ));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WeaponKind {
    Gauntlet,
    MachineGun,
    Railgun,
    RocketLauncher,
}

impl WeaponKind {
    /// Пауза между выстрелами при зажатой кнопке.
    fn cooldown(self) -> f32 {
        match self {
            WeaponKind::Gauntlet => 0.4,
            WeaponKind::MachineGun => 0.1, // 10 пуль в секунду
            WeaponKind::Railgun => 2.0,
            WeaponKind::RocketLauncher => 1.0,
        }
    }
}

#[derive(Component)]
pub struct Weapon {
    pub kind: WeaponKind,
    pub cooldown: f32,
}

impl Default for Weapon {
    fn default() -> Self {
        Self {
            kind: WeaponKind::MachineGun,
            cooldown: 0.0,
        }
    }
}

/// Физический снаряд: пуля автомата или ракета. Попадания обрабатывает `bullet_collision`.
#[derive(Component)]
pub struct Bullet {
    pub damage: f32,
    pub lifetime: f32,
    /// Радиус взрыва. Есть только у ракеты: она бьёт всех вокруг точки, куда упала.
    pub splash_radius: Option<f32>,
}

/// Первая поверхность или враг под прицелом: туда летит ракета.
#[derive(Resource, Default)]
pub struct AimPoint(pub Option<Vec3>);

/// Общие меши и материалы снарядов и вспышек: для трассировки это намного дешевле.
#[derive(Resource)]
pub struct WeaponAssets {
    bullet_mesh: Handle<Mesh>,
    bullet_material: Handle<StandardMaterial>,
    rocket_mesh: Handle<Mesh>,
    rocket_material: Handle<StandardMaterial>,
    /// Шар единичного радиуса, размер задаётся масштабом.
    sphere_mesh: Handle<Mesh>,
    /// Цилиндр единичной длины вдоль Y для следа рейлгана.
    trail_mesh: Handle<Mesh>,
}

/// Светящийся эффект, который гаснет и исчезает. У каждого свой материал, чтобы гаснуть отдельно.
#[derive(Component)]
struct Fading {
    remaining: f32,
    duration: f32,
    emissive: LinearRgba,
    material: Handle<StandardMaterial>,
}

/// Описание короткой светящейся вспышки-шара.
pub struct Flash {
    position: Vec3,
    radius: f32,
    emissive: LinearRgba,
    duration: f32,
}

impl Flash {
    pub fn explosion(position: Vec3, splash_radius: f32) -> Self {
        Self {
            position,
            radius: splash_radius * 0.5,
            emissive: LinearRgba::rgb(1.0, 0.45, 0.1) * 40_000.0,
            duration: 0.3,
        }
    }

    fn punch(position: Vec3) -> Self {
        Self {
            position,
            radius: 0.15,
            emissive: LinearRgba::rgb(1.0, 0.9, 0.7) * 20_000.0,
            duration: 0.1,
        }
    }
}

fn setup_weapon_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut glowing = |emissive: LinearRgba| materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive,
        ..default()
    });
    commands.insert_resource(WeaponAssets {
        bullet_mesh: meshes.add(raytraced(Sphere::new(0.15).mesh().ico(0).unwrap())),
        // Пуля сама светит и освещает арену
        bullet_material: glowing(LinearRgba::rgb(1.0, 0.8, 0.2) * 20_000.0),
        rocket_mesh: meshes.add(raytraced(Sphere::new(0.2).mesh().ico(1).unwrap())),
        rocket_material: glowing(LinearRgba::rgb(1.0, 0.35, 0.05) * 30_000.0),
        sphere_mesh: meshes.add(raytraced(Sphere::new(1.0).mesh().ico(2).unwrap())),
        trail_mesh: meshes.add(raytraced(Cylinder::new(RAIL_TRAIL_RADIUS, 1.0))),
    });
}

fn select_weapon(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut weapons: Query<&mut Weapon, With<Player>>,
) {
    let Ok(mut weapon) = weapons.single_mut() else {
        return;
    };
    let kind = if keyboard.just_pressed(KeyCode::Digit1) {
        WeaponKind::Gauntlet
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        WeaponKind::MachineGun
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        WeaponKind::Railgun
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        WeaponKind::RocketLauncher
    } else {
        return;
    };
    if weapon.kind != kind {
        weapon.kind = kind;
        info!("Оружие: {kind:?}");
    }
}

/// Разворачивает игрока к прицелу и ищет цель для ракеты.
///
/// Пули летят горизонтально на высоте ствола, поэтому игрок смотрит на точку, где луч
/// из-под прицела пересекает плоскость ствола. Тогда пуля на экране проходит ровно под
/// прицелом и на земле, и в прыжке, когда плоскость поднимается вместе с игроком.
fn aim_player(
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window>,
    rapier: ReadRapierContext,
    mut player: Query<&mut Transform, With<Player>>,
    mut aim: ResMut<AimPoint>,
) {
    aim.0 = None;
    let (Ok((camera, camera_transform)), Ok(window), Ok(mut player)) =
        (camera.single(), windows.single(), player.single_mut())
    else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };

    let muzzle = player.translation + Vec3::Y * MUZZLE_OFFSET_Y;
    if let Some(distance) = ray.intersect_plane(muzzle, InfinitePlane3d::new(Vec3::Y)) {
        let direction = (ray.get_point(distance) - muzzle).with_y(0.0);
        if direction.length_squared() > 1e-4 {
            player.look_to(direction, Vec3::Y);
        }
    }

    // Ракета: первое, во что упирается луч прицела, кроме самого игрока и снарядов
    let Ok(rapier) = rapier.single() else {
        return;
    };
    let filter = QueryFilter::exclude_kinematic().exclude_sensors();
    let direction = *ray.direction;
    aim.0 = rapier
        .cast_ray(ray.origin, direction, 500.0, true, filter)
        .map(|(_, toi)| ray.origin + direction * toi);
}

#[allow(clippy::too_many_arguments)]
fn handle_shooting(
    time: Res<Time>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    aim: Res<AimPoint>,
    assets: Res<WeaponAssets>,
    rapier: ReadRapierContext,
    mut player_query: Query<(&Transform, &mut Weapon), With<Player>>,
    mut enemies: Query<(Entity, &Transform, &mut Health, Option<&mut Armor>), With<Enemy>>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((player_transform, mut weapon)) = player_query.single_mut() else {
        return;
    };
    weapon.cooldown = (weapon.cooldown - time.delta_secs()).max(0.0);
    if !mouse_input.pressed(MouseButton::Left) || weapon.cooldown > 0.0 {
        return;
    }
    weapon.cooldown = weapon.kind.cooldown();

    // Игрок повёрнут к курсору, стреляем горизонтально вперёд
    let forward = player_transform.rotation * Vec3::NEG_Z;
    let muzzle = player_transform.translation + Vec3::Y * MUZZLE_OFFSET_Y;

    match weapon.kind {
        WeaponKind::Gauntlet => {
            punch(&mut commands, &assets, &mut materials, &mut enemies, muzzle, forward);
        }
        WeaponKind::MachineGun => {
            commands.spawn((
                projectile(
                    assets.bullet_mesh.clone(),
                    assets.bullet_material.clone(),
                    muzzle + forward * MUZZLE_DISTANCE,
                    forward * BULLET_SPEED,
                    0.15,
                ),
                Bullet {
                    damage: MACHINE_GUN_DAMAGE,
                    lifetime: BULLET_LIFETIME,
                    splash_radius: None,
                },
                Name::new("Bullet"),
            ));
        }
        WeaponKind::Railgun => {
            let Ok(rapier) = rapier.single() else {
                return;
            };
            rail_shot(&mut commands, &assets, &mut materials, &rapier, &mut enemies, muzzle, forward);
        }
        WeaponKind::RocketLauncher => {
            let start = muzzle + forward * MUZZLE_DISTANCE;
            // Ракета летит по прямой к точке под прицелом, снижаясь к ней от ствола.
            // Враг на пути перехватит её раньше. Без точки под курсором - в пол в 10 м впереди.
            let target = aim.0.unwrap_or(start.with_y(0.0) + forward * 10.0);
            let direction = (target - start).try_normalize().unwrap_or(forward);
            commands.spawn((
                projectile(
                    assets.rocket_mesh.clone(),
                    assets.rocket_material.clone(),
                    start,
                    direction * ROCKET_SPEED,
                    0.2,
                ),
                Bullet {
                    damage: ROCKET_DAMAGE,
                    lifetime: ROCKET_LIFETIME,
                    splash_radius: Some(ROCKET_SPLASH_RADIUS),
                },
                Name::new("Rocket"),
            ));
        }
    }
}

/// Летящий снаряд-сенсор: исчезает при касании чего угодно, кроме игрока.
fn projectile(
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    velocity: Vec3,
    radius: f32,
) -> impl Bundle {
    (
        Mesh3d(mesh.clone()),
        RaytracingMesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position),
        RigidBody::Dynamic,
        GravityScale(0.0), // Снаряды летят по прямой
        Collider::ball(radius),
        Sensor,
        ActiveEvents::COLLISION_EVENTS,
        Velocity {
            linear: velocity,
            angular: Vec3::ZERO,
        },
    )
}

/// Удар перчаткой: ближайший враг перед игроком в пределах досягаемости.
fn punch(
    commands: &mut Commands,
    assets: &WeaponAssets,
    materials: &mut Assets<StandardMaterial>,
    enemies: &mut Query<(Entity, &Transform, &mut Health, Option<&mut Armor>), With<Enemy>>,
    muzzle: Vec3,
    forward: Vec3,
) {
    let reach = PLAYER_RADIUS + GAUNTLET_REACH + ENEMY_RADIUS;
    let target = enemies
        .iter_mut()
        .filter_map(|(entity, transform, health, armor)| {
            let offset = (transform.translation - muzzle).with_y(0.0);
            let distance = offset.length();
            let in_front = offset.normalize_or_zero().dot(forward) > GAUNTLET_CONE_COS;
            let in_height = muzzle.y < enemy_top_y(armor.is_some());
            (distance < reach && in_front && in_height)
                .then_some((distance, entity, transform.translation, health, armor))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    if let Some((_, entity, position, mut health, armor)) = target {
        damage_enemy(commands, entity, &mut health, armor, Some(muzzle.y), GAUNTLET_DAMAGE);
        let hit_point = position.with_y(muzzle.y) - forward * ENEMY_RADIUS;
        spawn_flash(commands, assets, materials, Flash::punch(hit_point));
    }
}

/// Рейлган: мгновенный луч, прошивает всех врагов до первой стены и оставляет светящийся след.
fn rail_shot(
    commands: &mut Commands,
    assets: &WeaponAssets,
    materials: &mut Assets<StandardMaterial>,
    rapier: &RapierContext,
    enemies: &mut Query<(Entity, &Transform, &mut Health, Option<&mut Armor>), With<Enemy>>,
    muzzle: Vec3,
    forward: Vec3,
) {
    let length = rapier
        .cast_ray(muzzle, forward, RAIL_RANGE, true, QueryFilter::only_fixed())
        .map_or(RAIL_RANGE, |(_, toi)| toi);

    // Луч видит только врагов: пули тоже пересекаются с этой группой, поэтому сенсоры исключены
    let filter = QueryFilter::new()
        .exclude_sensors()
        .groups(CollisionGroups::new(Group::GROUP_3, ENEMY_GROUP));
    let mut hits = Vec::new();
    rapier.intersect_ray(muzzle, forward, length, true, filter, |entity, _, _| {
        if !hits.contains(&entity) {
            hits.push(entity);
        }
        true
    });
    for entity in hits {
        if let Ok((entity, _, mut health, armor)) = enemies.get_mut(entity) {
            damage_enemy(commands, entity, &mut health, armor, Some(muzzle.y), RAIL_DAMAGE);
        }
    }

    let emissive = LinearRgba::rgb(0.3, 0.8, 1.0) * 20_000.0;
    let material = glowing_material(materials, emissive);
    commands.spawn((
        Mesh3d(assets.trail_mesh.clone()),
        RaytracingMesh3d(assets.trail_mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(muzzle + forward * length * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, forward))
            .with_scale(Vec3::new(1.0, length, 1.0)),
        Fading {
            remaining: RAIL_TRAIL_DURATION,
            duration: RAIL_TRAIL_DURATION,
            emissive,
            material,
        },
        Name::new("Rail Trail"),
    ));
}

fn glowing_material(materials: &mut Assets<StandardMaterial>, emissive: LinearRgba) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive,
        ..default()
    })
}

/// Короткая гаснущая вспышка: взрыв ракеты, удар перчаткой.
pub fn spawn_flash(
    commands: &mut Commands,
    assets: &WeaponAssets,
    materials: &mut Assets<StandardMaterial>,
    flash: Flash,
) {
    let material = glowing_material(materials, flash.emissive);
    commands.spawn((
        Mesh3d(assets.sphere_mesh.clone()),
        RaytracingMesh3d(assets.sphere_mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(flash.position).with_scale(Vec3::splat(flash.radius)),
        Fading {
            remaining: flash.duration,
            duration: flash.duration,
            emissive: flash.emissive,
            material,
        },
        Name::new("Flash"),
    ));
}

fn fade_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut effects: Query<(Entity, &mut Fading)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut fading) in effects.iter_mut() {
        fading.remaining -= time.delta_secs();
        if fading.remaining <= 0.0 {
            materials.remove(&fading.material);
            commands.entity(entity).despawn();
        } else if let Some(mut material) = materials.get_mut(&fading.material) {
            material.emissive = fading.emissive * (fading.remaining / fading.duration);
        }
    }
}

fn update_bullets(
    time: Res<Time>,
    mut bullet_query: Query<(Entity, &mut Bullet)>,
    mut commands: Commands,
) {
    for (entity, mut bullet) in bullet_query.iter_mut() {
        bullet.lifetime -= time.delta_secs();
        if bullet.lifetime <= 0.0 {
            commands.entity(entity).try_despawn(); // В этом же кадре пулю могло уже снять попадание
        }
    }
}

// Система для добавления оружия игроку после его создания
pub fn equip_player_weapon(
    mut commands: Commands,
    player_query: Query<Entity, (With<Player>, Without<Weapon>)>,
) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity).insert(Weapon::default());
    }
}
