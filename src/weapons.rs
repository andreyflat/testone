// src/weapons.rs - Стрельба вперед по направлению игрока
use bevy::prelude::*;
use bevy::solari::prelude::RaytracingMesh3d;
use bevy_rapier3d::prelude::*;
use crate::enemies::ENEMY_CENTER_Y;
use crate::player::Player;

pub struct WeaponsPlugin;
impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
               handle_shooting,
               update_bullets,
           ));
    }
}

#[derive(Component)]
pub struct Weapon {
    pub damage: f32,
    pub fire_rate: f32,      // выстрелов в секунду
    pub last_shot_time: f32,
}

impl Default for Weapon {
    fn default() -> Self {
        Self {
            damage: 25.0,
            fire_rate: 10.0,     // 10 выстрелов в секунду
            last_shot_time: 0.0,
        }
    }
}

#[derive(Component)]
pub struct Bullet {
    //pub velocity: Vec3,
    pub damage: f32,
    pub lifetime: f32,
}

pub fn handle_shooting(
    time: Res<Time>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut player_query: Query<(&Transform, &mut Weapon), With<Player>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bullet_assets: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    let Ok((player_transform, mut weapon)) = player_query.single_mut() else {
        return;
    };

    // Проверяем, можем ли стрелять (cooldown)
    let current_time = time.elapsed_secs();
    let time_since_last_shot = current_time - weapon.last_shot_time;
    let can_shoot = time_since_last_shot >= (1.0 / weapon.fire_rate);

    if mouse_input.pressed(MouseButton::Left) && can_shoot {
        // Стреляем в направлении "вперед" игрока
        // В Bevy forward направление это -Z (Vec3::NEG_Z)
        let forward_direction = player_transform.rotation * Vec3::NEG_Z;
        
        // Пуля появляется перед игроком на высоте врагов: стрельба в прыжке тоже попадает
        let mut spawn_position = player_transform.translation + forward_direction * 1.0;
        spawn_position.y = ENEMY_CENTER_Y;


        // Один меш и материал на все пули: для трассировки это намного дешевле
        let (mesh, material) = bullet_assets.get_or_insert_with(|| (
            meshes.add(crate::lights::raytraced(Sphere::new(0.15).mesh().ico(0).unwrap())),
            materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 1.0, 0.0), // Желтые пули
                emissive: LinearRgba::rgb(1.0, 0.8, 0.2) * 20_000.0, // Пуля сама светит и освещает арену
                ..default()
            }),
        ));

        // Создаем пулю
        spawn_bullet(
            &mut commands,
            mesh.clone(),
            material.clone(),
            spawn_position,
            forward_direction,
            weapon.damage,
        );
        
        weapon.last_shot_time = current_time;
    }
}

fn spawn_bullet(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    position: Vec3,
    direction: Vec3,
    damage: f32,
) {
    let bullet_speed = 50.0;
    
    commands.spawn((
        Mesh3d(mesh.clone()),
        RaytracingMesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(position),
        Bullet {
            //velocity: direction * bullet_speed,
            damage,
            lifetime: 2.0, // пуля живет 2 секунды
        },
        RigidBody::Dynamic,
        GravityScale(0.0), // Пуля летит ровно по прицелу
        Collider::ball(0.15),
        Sensor, // пуля проходит сквозь объекты, но регистрирует столкновения
        ActiveEvents::COLLISION_EVENTS,
        Velocity {
            linear: direction * bullet_speed,
            angular: Vec3::ZERO,
        },
        Name::new("Bullet"),
    ));
}

fn update_bullets(
    time: Res<Time>,
    mut bullet_query: Query<(Entity, &mut Bullet)>,
    mut commands: Commands,
) {
    for (entity, mut bullet) in bullet_query.iter_mut() {
        // Двигаем пулю
        //transform.translation += bullet.velocity * time.delta_secs();
        // Уменьшаем время жизни
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
