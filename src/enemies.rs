// src/enemies.rs - Полная система врагов
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use crate::player::Player;
use crate::weapons::Bullet;

pub struct EnemiesPlugin;
impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
                enemy_ai_system,
                bullet_enemy_collision,
                enemy_death_system,
            ))
           .add_systems(Startup, spawn_initial_enemies);
    }
}

#[derive(Component)]
pub struct Enemy {
    pub speed: f32,
    pub target: Option<Entity>,
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

fn spawn_initial_enemies(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Спавним несколько врагов вокруг игрока
    let spawn_positions = vec![
        Vec3::new(5.0, 1.0, 5.0),
        Vec3::new(-5.0, 1.0, 5.0),
        Vec3::new(5.0, 1.0, -5.0),
    ];
    
    for pos in spawn_positions {
        spawn_enemy(&mut commands, &mut meshes, &mut materials, pos);
    }
}

pub fn spawn_enemy(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    position: Vec3,
) {
    commands.spawn((
        // Визуал - красный куб
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2), // Красный цвет
            ..default()
        })),
        Transform::from_translation(position),
        
        // Физика
        RigidBody::Dynamic,
        Collider::cuboid(0.5, 0.5, 0.5),
        LockedAxes::ROTATION_LOCKED, // Не позволяем врагу вращаться
        
        // Компоненты врага
        Enemy {
            speed: 3.0,
            target: None,
        },
        Health::new(50.0),
        
        Name::new("Enemy"),
    ));
}

fn enemy_ai_system(
    time: Res<Time>,
    player_query: Query<Entity, With<Player>>,
    player_transform_query: Query<&Transform, With<Player>>,
    mut enemy_query: Query<(&mut Transform, &mut Enemy), (Without<Player>, With<Enemy>)>,
) {
    let Ok(player_entity) = player_query.get_single() else {
        return;
    };
    
    let Ok(player_transform) = player_transform_query.get_single() else {
        return;
    };
    
    for (mut enemy_transform, mut enemy) in enemy_query.iter_mut() {
        // Устанавливаем цель (игрока)
        enemy.target = Some(player_entity);
        
        // Вычисляем направление к игроку
        let direction = (player_transform.translation - enemy_transform.translation).normalize();
        
        // Двигаемся к игроку (только по X и Z, игнорируем Y)
        let movement = Vec3::new(
            direction.x * enemy.speed * time.delta_secs(),
            0.0,
            direction.z * enemy.speed * time.delta_secs(),
        );
        
        enemy_transform.translation += movement;
    }
}

fn bullet_enemy_collision(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    bullet_query: Query<&Bullet>,
    mut enemy_query: Query<&mut Health, With<Enemy>>,
) {
    for collision_event in collision_events.read() {
        if let CollisionEvent::Started(entity1, entity2, _) = collision_event {
            // Проверяем, столкнулись ли пуля и враг
            let (bullet_entity, enemy_entity) = if bullet_query.contains(*entity1) {
                (*entity1, *entity2)
            } else if bullet_query.contains(*entity2) {
                (*entity2, *entity1)
            } else {
                continue;
            };
            
            // Получаем урон от пули
            if let Ok(bullet) = bullet_query.get(bullet_entity) {
                // Наносим урон врагу
                if let Ok(mut enemy_health) = enemy_query.get_mut(enemy_entity) {
                    enemy_health.take_damage(bullet.damage);
                    
                    // Удаляем пулю
                    commands.entity(bullet_entity).despawn();
                }
            }
        }
    }
}

fn enemy_death_system(
    mut commands: Commands,
    enemy_query: Query<(Entity, &Health), With<Enemy>>,
) {
    for (enemy_entity, health) in enemy_query.iter() {
        if health.is_dead() {
            commands.entity(enemy_entity).despawn();
        }
    }
}