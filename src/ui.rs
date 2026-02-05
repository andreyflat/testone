// src/ui.rs - UI без crosshair (так как теперь есть курсор на земле)
use bevy::prelude::*;
use bevy_rapier3d::prelude::CollisionEvent;
use crate::player::Player;
use crate::enemies::{Enemy, Health};

pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_ui, setup_player_health))
           .add_systems(Update, (
               update_health_bar,
               update_enemy_counter,
               player_enemy_collision,
           ));
    }
}

#[derrive(Component)]
struct HealthBar;

#[derive(Component)]
struct EnemyCounter;

fn setup_ui(mut commands: Commands) {
    // UI Root
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            // Health Bar Background
            parent
                .spawn(NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left: Val::Px(20.0),
                        top: Val::Px(20.0),
                        width: Val::Px(200.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    background_color: BackgroundColor(Color::srgb(0.2, 0.2, 0.2)),
                    ..default()
                })
                .with_children(|parent| {
                    // Health Bar Fill
                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            background_color: BackgroundColor(Color::srgb(0.8, 0.2, 0.2)),
                            ..default()
                        },
                        HealthBar,
                    ));
                });
            
            // Enemy Counter
            parent.spawn((
                TextBundle::from_section(
                    "Enemies: 0",
                    TextStyle {
                        font_size: 24.0,
                        color: Color::WHITE,
                        ..default()
                    },
                ).with_style(Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(20.0),
                    top: Val::Px(50.0),
                    ..default()
                }),
                EnemyCounter,
            ));
            
            // Инструкции управления
            parent.spawn(
                TextBundle::from_section(
                    "WASD - движение, Space - прыжок, ЛКМ - стрельба",
                    TextStyle {
                        font_size: 16.0,
                        color: Color::srgba(1.0, 1.0, 1.0, 0.7),
                        ..default()
                    },
                ).with_style(Style {
                    position_type: PositionType::Absolute,
                    right: Val::Px(20.0),
                    bottom: Val::Px(20.0),
                    ..default()
                }),
            );
        });
}

fn setup_player_health(
    mut commands: Commands,
    player_query: Query<Entity, (With<Player>, Without<Health>)>,
) {
    for player_entity in player_query.iter() {
        commands.entity(player_entity).insert(Health::new(100.0));
    }
}

fn update_health_bar(
    player_query: Query<&Health, With<Player>>,
    mut health_bar_query: Query<&mut Style, With<HealthBar>>,
) {
    if let Ok(health) = player_query.get_single() {
        if let Ok(mut style) = health_bar_query.get_single_mut() {
            let health_percentage = health.current / health.max;
            style.width = Val::Percent(health_percentage * 100.0);
        }
    }
}

fn update_enemy_counter(
    enemy_query: Query<&Enemy>,
    mut counter_query: Query<&mut Text, With<EnemyCounter>>,
) {
    let enemy_count = enemy_query.iter().count();
    
    if let Ok(mut text) = counter_query.get_single_mut() {
        text.sections[0].value = format!("Enemies: {}", enemy_count);
    }
}

fn player_enemy_collision(
    mut collision_events: EventReader<CollisionEvent>,
    player_query: Query<Entity, With<Player>>,
    mut player_health_query: Query<&mut Health, With<Player>>,
    enemy_query: Query<Entity, With<Enemy>>,
) {
    let Ok(player_entity) = player_query.get_single() else {
        return;
    };
    
    for collision_event in collision_events.read() {
        if let CollisionEvent::Stopped(entity1, entity2, _) = collision_event {
            let enemy_entity = if *entity1 == player_entity && enemy_query.contains(*entity2) {
                Some(*entity2)
            } else if *entity2 == player_entity && enemy_query.contains(*entity1) {
                Some(*entity1)
            } else {
                None
            };
            
            if enemy_entity.is_some() {
                if let Ok(mut health) = player_health_query.get_single_mut() {
                    health.take_damage(10.0);
                    
                    if health.is_dead() {
                        println!("Game Over! Используй стрейф-джампинг для уклонения!");
                    }
                }
            }
        }
    }
}
