// src/weapons.rs - Оружие игрока: перчатка, автомат, рейлган и ракетница
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use dummies::DamageSystems;
use crate::enemies::{ENEMY_GROUP, ENEMY_HEIGHT, ENEMY_RADIUS, Enemy, EnemyHits, Health, PLAYER_GROUP, RAGDOLL_GROUP};
use crate::lights::raytraced;
use crate::player::{Player, PlayerCamera, Velocity as PlayerVelocity};

/// Ствол на высоте плеча, над центром игрока (около 1.4 м над полом). Выстрел без врага
/// под прицелом идёт горизонтально на этой высоте: с пола в грудь врагу и поверх ящиков.
/// Ракета с такой высоты не стелется по полу на пути к дальней точке на нём.
const MUZZLE_OFFSET_Y: f32 = 0.55;
/// Удар перчаткой идёт ниже, от пояса: иначе в прыжке кулак оказывался бы выше врага.
const PUNCH_OFFSET_Y: f32 = -0.32;
/// Близко к игроку, иначе пуля появляется уже за врагом, стоящим вплотную.
const MUZZLE_DISTANCE: f32 = 0.4;
const PLAYER_RADIUS: f32 = 0.3;
/// За столько секунд модель игрока проходит примерно две трети поворота к прицелу.
const TURN_LAG: f32 = 0.06;

const GAUNTLET_DAMAGE: f32 = 50.0;
/// Расстояние от поверхности игрока до поверхности врага, на котором достаёт удар.
const GAUNTLET_REACH: f32 = 0.6;
/// Косинус половины угла перед игроком, в котором действует удар.
const GAUNTLET_CONE_COS: f32 = 0.5;

const MACHINE_GUN_DAMAGE: f32 = 5.0;
/// Сила толчка от пули, от 0 до 1. Одна пуля автомата дёргает ту часть тела, в которую попала,
/// а очередь из четырёх-пяти подряд сбивает врага с шага. Перчатка, рейлган и взрыв сбивают сразу.
const MACHINE_GUN_IMPACT: f32 = 0.3;
const BULLET_RADIUS: f32 = 0.15;
const ROCKET_RADIUS: f32 = 0.2;
const BULLET_SPEED: f32 = 50.0;
const BULLET_LIFETIME: f32 = 2.0;

const RAIL_DAMAGE: f32 = 80.0;
/// Попадание рейлгана в голову убивает сразу: это вдвое больше здоровья врага.
const RAIL_HEADSHOT_DAMAGE: f32 = 200.0;
const RAIL_RANGE: f32 = 80.0;
const RAIL_TRAIL_DURATION: f32 = 0.5;
const RAIL_TRAIL_RADIUS: f32 = 0.03;

const ROCKET_DAMAGE: f32 = 100.0;
const ROCKET_SPEED: f32 = 20.0;
/// След ракеты: гаснущие огоньки, по одному на такой отрезок пути в метрах.
const ROCKET_TRAIL_SPACING: f32 = 0.5;
const ROCKET_SPLASH_RADIUS: f32 = 1.5;
const ROCKET_LIFETIME: f32 = 6.0;
/// Рокетджамп: скорость, которую взрыв вплотную придаёт игроку. Урона игроку взрыв не наносит.
/// Для сравнения, обычный прыжок - 6.75 м/с.
const ROCKET_JUMP_SPEED: f32 = 8.0;
/// Взрыв толкает игрока на расстоянии до двух радиусов поражения, как и тела врагов.
const ROCKET_JUMP_REACH: f32 = ROCKET_SPLASH_RADIUS * 2.0;
/// Половина высоты цилиндрической части капсулы игрока.
const PLAYER_HALF_HEIGHT: f32 = 0.5;

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AimPoint>()
            .add_systems(Startup, setup_weapon_assets)
            // Попадания идут раньше, чем тела врагов принимают урон: убитый этим кадром
            // сразу переходит к физике, и толчок от взрыва достаётся уже рэгдоллу
            .add_systems(Update, (
                (select_weapon, aim_player, handle_shooting, fly_projectiles).chain().before(DamageSystems),
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

/// Летящий снаряд: пуля автомата или ракета. Двигает его и ищет попадания `fly_projectiles`.
#[derive(Component)]
pub struct Bullet {
    pub damage: f32,
    pub lifetime: f32,
    pub velocity: Vec3,
    pub radius: f32,
    /// Радиус взрыва. Есть только у ракеты: она бьёт всех вокруг точки, куда упала.
    pub splash_radius: Option<f32>,
    /// Сколько метров пролетел с последнего огонька следа. Есть только у ракеты.
    pub trail: Option<f32>,
    /// Пуля врага: летит сквозь врагов и ранит игрока.
    pub hostile: bool,
}

/// Что находится под прицелом.
#[derive(Resource, Default)]
pub struct AimPoint {
    /// Первая поверхность или враг под прицелом: туда летит ракета.
    pub surface: Option<Vec3>,
    /// Точка на теле живого врага под прицелом: туда летят пуля и луч рейлгана.
    pub enemy: Option<Vec3>,
    /// Горизонтальное направление от игрока к прицелу. Выстрел идёт по нему сразу,
    /// а модель игрока доворачивается с небольшим отставанием.
    pub forward: Option<Vec3>,
}

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
    enemy_bullet_mesh: Handle<Mesh>,
    enemy_bullet_material: Handle<StandardMaterial>,
}

/// Пуля врага: красная, меньше и медленнее пули игрока, от неё можно уйти.
pub const ENEMY_BULLET_SPEED: f32 = 22.0;
const ENEMY_BULLET_RADIUS: f32 = 0.08;
const ENEMY_BULLET_DAMAGE: f32 = 5.0;

/// Выпускает пулю врага из `position` в сторону `direction`.
pub fn spawn_enemy_bullet(commands: &mut Commands, assets: &WeaponAssets, position: Vec3, direction: Vec3) {
    commands.spawn((
        projectile(assets.enemy_bullet_mesh.clone(), assets.enemy_bullet_material.clone(), position),
        Bullet {
            damage: ENEMY_BULLET_DAMAGE,
            lifetime: BULLET_LIFETIME,
            velocity: direction * ENEMY_BULLET_SPEED,
            radius: ENEMY_BULLET_RADIUS,
            splash_radius: None,
            trail: None,
            hostile: true,
        },
        Name::new("Enemy Bullet"),
    ));
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

    /// Огонёк в следе ракеты.
    fn trail(position: Vec3) -> Self {
        Self {
            position,
            radius: 0.09,
            emissive: LinearRgba::rgb(1.0, 0.5, 0.15) * 6_000.0,
            duration: 0.45,
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
        bullet_mesh: meshes.add(raytraced(Sphere::new(BULLET_RADIUS).mesh().ico(0).unwrap())),
        // Пуля сама светит и освещает арену
        bullet_material: glowing(LinearRgba::rgb(1.0, 0.8, 0.2) * 20_000.0),
        rocket_mesh: meshes.add(raytraced(Sphere::new(ROCKET_RADIUS).mesh().ico(1).unwrap())),
        rocket_material: glowing(LinearRgba::rgb(1.0, 0.35, 0.05) * 30_000.0),
        sphere_mesh: meshes.add(raytraced(Sphere::new(1.0).mesh().ico(2).unwrap())),
        trail_mesh: meshes.add(raytraced(Cylinder::new(RAIL_TRAIL_RADIUS, 1.0))),
        enemy_bullet_mesh: meshes.add(raytraced(Sphere::new(ENEMY_BULLET_RADIUS).mesh().ico(0).unwrap())),
        enemy_bullet_material: glowing(LinearRgba::rgb(1.0, 0.1, 0.05) * 20_000.0),
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

/// Разворачивает игрока к прицелу и ищет, что под ним находится.
///
/// Прицел на враге: выстрел идёт от ствола прямо в ту точку тела, что видна под прицелом.
/// Прицел на полу или стене: пули летят горизонтально на высоте ствола, и игрок смотрит
/// на точку, где луч из-под прицела пересекает плоскость ствола. Тогда пуля на экране
/// проходит ровно под прицелом и на земле, и в прыжке.
fn aim_player(
    time: Res<Time>,
    camera: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window>,
    rapier: ReadRapierContext,
    mut player: Query<&mut Transform, With<Player>>,
    groups: Query<&CollisionGroups>,
    mut aim: ResMut<AimPoint>,
) {
    *aim = AimPoint::default();
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

    // Первое, во что упирается луч прицела, кроме самого игрока
    if let Ok(rapier) = rapier.single() {
        let filter = QueryFilter::new().exclude_sensors().groups(not_player());
        let direction = *ray.direction;
        if let Some((collider, toi)) = rapier.cast_ray(ray.origin, direction, 500.0, true, filter) {
            let point = ray.origin + direction * toi;
            aim.surface = Some(point);
            let alive = groups.get(collider).is_ok_and(|groups| groups.memberships.contains(ENEMY_GROUP));
            if alive {
                aim.enemy = Some(point);
            }
        }
    }

    let muzzle = player.translation + Vec3::Y * MUZZLE_OFFSET_Y;
    let target = aim
        .enemy
        .or_else(|| ray.intersect_plane(muzzle, InfinitePlane3d::new(Vec3::Y)).map(|distance| ray.get_point(distance)));
    if let Some(target) = target {
        let direction = (target - muzzle).with_y(0.0);
        if direction.length_squared() > 1e-4 {
            aim.forward = direction.try_normalize();
            // Модель доворачивается плавно: иначе она дёргается, когда прицел
            // переходит с пола на врага и точка прицеливания прыгает
            let facing = Transform::default().looking_to(direction, Vec3::Y).rotation;
            let blend = 1.0 - (-time.delta_secs() / TURN_LAG).exp();
            player.rotation = player.rotation.slerp(facing, blend);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_shooting(
    time: Res<Time>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    aim: Res<AimPoint>,
    assets: Res<WeaponAssets>,
    rapier: ReadRapierContext,
    mut player_query: Query<(&Transform, &mut Weapon), With<Player>>,
    enemies: Query<(Entity, &Enemy)>,
    mut hits: EnemyHits,
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

    // Игрок повёрнут к курсору. Пуля и луч идут в точку на враге под прицелом,
    // а если под прицелом врага нет - горизонтально вперёд
    let forward = aim.forward.unwrap_or(player_transform.rotation * Vec3::NEG_Z);
    let muzzle = player_transform.translation + Vec3::Y * MUZZLE_OFFSET_Y;
    let shot = aim
        .enemy
        .and_then(|point| (point - muzzle).try_normalize())
        .unwrap_or(forward);

    match weapon.kind {
        WeaponKind::Gauntlet => {
            let fist = player_transform.translation + Vec3::Y * PUNCH_OFFSET_Y;
            punch(&mut commands, &assets, &mut materials, &enemies, &mut hits, fist, forward);
        }
        WeaponKind::MachineGun => {
            commands.spawn((
                projectile(
                    assets.bullet_mesh.clone(),
                    assets.bullet_material.clone(),
                    muzzle + shot * MUZZLE_DISTANCE,
                ),
                Bullet {
                    damage: MACHINE_GUN_DAMAGE,
                    lifetime: BULLET_LIFETIME,
                    velocity: shot * BULLET_SPEED,
                    radius: BULLET_RADIUS,
                    splash_radius: None,
                    trail: None,
                    hostile: false,
                },
                Name::new("Bullet"),
            ));
        }
        WeaponKind::Railgun => {
            let Ok(rapier) = rapier.single() else {
                return;
            };
            rail_shot(&mut commands, &assets, &mut materials, &rapier, &mut hits, muzzle, shot);
        }
        WeaponKind::RocketLauncher => {
            let start = muzzle + forward * MUZZLE_DISTANCE;
            // Ракета летит по прямой к точке под прицелом, снижаясь к ней от плеча.
            // Враг на пути перехватит её раньше. Без точки под курсором - в пол в 10 м впереди.
            let target = aim.surface.unwrap_or(start.with_y(0.0) + forward * 10.0);
            let direction = (target - start).try_normalize().unwrap_or(forward);
            commands.spawn((
                projectile(
                    assets.rocket_mesh.clone(),
                    assets.rocket_material.clone(),
                    start,
                ),
                Bullet {
                    damage: ROCKET_DAMAGE,
                    lifetime: ROCKET_LIFETIME,
                    velocity: direction * ROCKET_SPEED,
                    radius: ROCKET_RADIUS,
                    splash_radius: Some(ROCKET_SPLASH_RADIUS),
                    trail: Some(0.0),
                    hostile: false,
                },
                Name::new("Rocket"),
            ));
        }
    }
}

/// Всё, кроме игрока: во что попадают снаряды и упирается прицел.
fn not_player() -> CollisionGroups {
    CollisionGroups::new(Group::ALL, Group::ALL ^ PLAYER_GROUP)
}

/// Всё, кроме врагов и их тел: во что попадают пули врагов.
fn not_enemies() -> CollisionGroups {
    CollisionGroups::new(Group::ALL, Group::ALL ^ (ENEMY_GROUP | RAGDOLL_GROUP))
}

/// Вид снаряда. Физического тела у него нет: его двигает `fly_projectiles`.
fn projectile(mesh: Handle<Mesh>, material: Handle<StandardMaterial>, position: Vec3) -> impl Bundle {
    (
        Mesh3d(mesh.clone()),
        RaytracingMesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position),
    )
}

/// Двигает снаряды по прямой и ищет, во что они попали, проводя шар снаряда вдоль всего пути
/// за кадр: быстрая пуля не проскочит ни тонкую руку врага, ни перегородку.
/// Снаряд исчезает при касании чего угодно, кроме игрока. Урон получают только враги.
fn fly_projectiles(
    time: Res<Time>,
    rapier: ReadRapierContext,
    assets: Res<WeaponAssets>,
    mut bullets: Query<(Entity, &mut Bullet, &mut Transform)>,
    mut player: Query<(Entity, &Transform, &mut PlayerVelocity, &mut Health), (With<Player>, Without<Bullet>)>,
    mut hits: EnemyHits,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(rapier) = rapier.single() else {
        return;
    };
    let dt = time.delta_secs();
    for (entity, mut bullet, mut transform) in bullets.iter_mut() {
        bullet.lifetime -= dt;
        if bullet.lifetime <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(direction) = bullet.velocity.try_normalize() else {
            continue;
        };
        let start = transform.translation;
        let options = ShapeCastOptions {
            max_time_of_impact: bullet.velocity.length() * dt,
            target_distance: 0.0,
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: false,
        };
        let groups = if bullet.hostile { not_enemies() } else { not_player() };
        let filter = QueryFilter::new().exclude_sensors().groups(groups);
        let shape = Collider::ball(bullet.radius);
        let Some((collider, hit)) = rapier.cast_shape(start, Quat::IDENTITY, direction, &shape, options, filter) else {
            let step = bullet.velocity.length() * dt;
            transform.translation = start + direction * step;
            if let Some(travelled) = &mut bullet.trail {
                *travelled += step;
                if *travelled >= ROCKET_TRAIL_SPACING {
                    *travelled = 0.0;
                    spawn_flash(&mut commands, &assets, &mut materials, Flash::trail(start));
                }
            }
            continue;
        };
        commands.entity(entity).despawn();
        let position = start + direction * hit.time_of_impact;

        if bullet.hostile {
            if let Ok((player, _, _, mut health)) = player.single_mut()
                && collider == player
            {
                health.take_damage(bullet.damage);
                info!("Игрок получил урон от пули, здоровье: {}", health.current);
            }
        } else if let Some(radius) = bullet.splash_radius {
            // Ракета взрывается о любую поверхность и задевает всех врагов в радиусе
            spawn_flash(&mut commands, &assets, &mut materials, Flash::explosion(position, radius));
            hits.explode(position, radius, bullet.damage);
            if let Ok((_, player, mut velocity, _)) = player.single_mut() {
                velocity.0 += blast_push(position, player.translation);
            }
        } else if let Some((body, part)) = hits.target(collider) {
            let point = position + direction * bullet.radius;
            hits.hit(body, part, point, direction, bullet.damage, MACHINE_GUN_IMPACT, Some(position.y));
        }
    }
}

/// Скорость, которую взрыв в точке `origin` придаёт игроку с центром в `player`.
/// Толчок идёт от взрыва к центру игрока и слабеет с расстоянием до поверхности капсулы.
fn blast_push(origin: Vec3, player: Vec3) -> Vec3 {
    let offset = origin - player;
    let nearest = player + Vec3::Y * offset.y.clamp(-PLAYER_HALF_HEIGHT, PLAYER_HALF_HEIGHT);
    let distance = (origin.distance(nearest) - PLAYER_RADIUS).max(0.0);
    let falloff = (1.0 - distance / ROCKET_JUMP_REACH).max(0.0);
    (player - origin).try_normalize().unwrap_or(Vec3::Y) * ROCKET_JUMP_SPEED * falloff
}

/// Удар перчаткой: ближайший враг перед игроком в пределах досягаемости.
fn punch(
    commands: &mut Commands,
    assets: &WeaponAssets,
    materials: &mut Assets<StandardMaterial>,
    enemies: &Query<(Entity, &Enemy)>,
    hits: &mut EnemyHits,
    muzzle: Vec3,
    forward: Vec3,
) {
    let reach = PLAYER_RADIUS + GAUNTLET_REACH + ENEMY_RADIUS;
    let target = enemies
        .iter()
        .filter_map(|(entity, enemy)| {
            let offset = (enemy.position - muzzle).with_y(0.0);
            let distance = offset.length();
            let in_front = offset.normalize_or_zero().dot(forward) > GAUNTLET_CONE_COS;
            let in_height = muzzle.y < ENEMY_HEIGHT;
            (distance < reach && in_front && in_height).then_some((distance, entity, enemy.position))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    if let Some((_, entity, position)) = target {
        // Удар приходится в ту часть тела, что ближе всего к кулаку
        let fist = position.with_y(muzzle.y) - forward * ENEMY_RADIUS;
        let Some((part, place)) = hits.nearest_part(entity, fist) else {
            return;
        };
        hits.hit(entity, part, place, forward, GAUNTLET_DAMAGE, 1.0, Some(muzzle.y));
        spawn_flash(commands, assets, materials, Flash::punch(fist));
    }
}

/// Рейлган: мгновенный луч, прошивает всех врагов до первой стены и оставляет светящийся след.
fn rail_shot(
    commands: &mut Commands,
    assets: &WeaponAssets,
    materials: &mut Assets<StandardMaterial>,
    rapier: &RapierContext,
    hits: &mut EnemyHits,
    muzzle: Vec3,
    forward: Vec3,
) {
    let length = rapier
        .cast_ray(muzzle, forward, RAIL_RANGE, true, QueryFilter::only_fixed())
        .map_or(RAIL_RANGE, |(_, toi)| toi);

    // Луч видит только части тел живых врагов. Каждому врагу достаётся одно попадание:
    // в ту часть тела, которую луч встретил первой
    let filter = QueryFilter::new()
        .exclude_sensors()
        .groups(CollisionGroups::new(Group::ALL, ENEMY_GROUP));
    let mut struck: Vec<(Entity, Entity, f32)> = Vec::new();
    rapier.intersect_ray(muzzle, forward, length, true, filter, |collider, _, intersection| {
        if let Some((body, part)) = hits.target(collider) {
            let distance = intersection.time_of_impact;
            match struck.iter_mut().find(|hit| hit.0 == body) {
                Some(hit) if distance < hit.2 => *hit = (body, part, distance),
                Some(_) => {}
                None => struck.push((body, part, distance)),
            }
        }
        true
    });
    for (body, part, distance) in struck {
        let point = muzzle + forward * distance;
        let damage = if hits.is_head(part) { RAIL_HEADSHOT_DAMAGE } else { RAIL_DAMAGE };
        hits.hit(body, part, point, forward, damage, 1.0, Some(point.y));
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

// Система для добавления оружия игроку после его создания
pub fn equip_player_weapon(
    mut commands: Commands,
    player_query: Query<Entity, (With<Player>, Without<Weapon>)>,
) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity).insert(Weapon::default());
    }
}
